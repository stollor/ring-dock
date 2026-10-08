use serde::Deserialize;
use std::ffi::c_void;
use std::time::Instant;
use windows::core::{w, PCWSTR};
use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator, Geoposition};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE,
};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus};

#[derive(Clone, Copy, Debug)]
pub struct SystemLocation {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug)]
pub struct EnvironmentSnapshot {
    pub location: SystemLocation,
    pub sunrise_minute: Option<u16>,
    pub sunset_minute: Option<u16>,
    pub weather_code: i32,
    pub is_day: bool,
    pub cloud_cover: f32,
    pub wind_speed_kmh: f32,
    pub wind_direction: f32,
    pub updated_at: Instant,
}

/// The permission prompt must be requested from the foreground UI thread. Its completion is
/// asynchronous so the desktop and settings windows keep processing messages while it appears.
pub fn request_system_location_async(hwnd: HWND, message: u32) -> Result<(), String> {
    let access = Geolocator::RequestAccessAsync()
        .map_err(|error| format!("启动 Windows 定位授权失败：{error}"))?;
    let target = hwnd.0 as isize;
    let access_handler = AsyncOperationCompletedHandler::<GeolocationAccessStatus>::new(
        move |operation, status| {
            let access_result = if status != AsyncStatus::Completed {
                Err("Windows 定位授权没有完成，请检查系统定位设置".to_string())
            } else {
                operation
                    .ok()
                    .map_err(|error| format!("读取 Windows 定位授权结果失败：{error}"))
                    .and_then(|operation| {
                        operation
                            .GetResults()
                            .map_err(|error| format!("读取 Windows 定位授权结果失败：{error}"))
                    })
                    .and_then(|access| match access {
                        GeolocationAccessStatus::Allowed => Ok(()),
                        GeolocationAccessStatus::Denied => Err(
                            "Windows 已拒绝定位权限，请在“设置 → 隐私和安全性 → 位置”中允许桌面应用访问位置".into(),
                        ),
                        _ => Err("Windows 没有授予定位权限，请检查系统定位设置".into()),
                    })
            };
            if let Err(error) = access_result {
                post_location_result(HWND(target as *mut _), message, Err(error));
                return Ok(());
            }

            let locator = match Geolocator::new() {
                Ok(locator) => locator,
                Err(error) => {
                    post_location_result(
                        HWND(target as *mut _),
                        message,
                        Err(format!("创建 Windows 定位服务失败：{error}")),
                    );
                    return Ok(());
                }
            };
            let position = match locator.GetGeopositionAsync() {
                Ok(position) => position,
                Err(error) => {
                    post_location_result(
                        HWND(target as *mut _),
                        message,
                        Err(format!("启动系统位置读取失败：{error}")),
                    );
                    return Ok(());
                }
            };
            let position_target = target;
            let position_handler =
                AsyncOperationCompletedHandler::<Geoposition>::new(move |operation, status| {
                    let result = if status == AsyncStatus::Completed {
                        operation
                            .ok()
                            .map_err(|error| format!("读取系统位置失败：{error}"))
                            .and_then(|operation| {
                                operation
                                    .GetResults()
                                    .map_err(|error| format!("读取系统位置失败：{error}"))
                            })
                            .and_then(location_from_position)
                    } else {
                        Err("Windows 未能返回当前位置，请检查定位服务是否开启".into())
                    };
                    post_location_result(HWND(position_target as *mut _), message, result);
                    Ok(())
                });
            if let Err(error) = position.SetCompleted(&position_handler) {
                post_location_result(
                    HWND(target as *mut _),
                    message,
                    Err(format!("等待系统位置失败：{error}")),
                );
            }
            Ok(())
        },
    );
    access
        .SetCompleted(&access_handler)
        .map_err(|error| format!("注册 Windows 定位授权回调失败：{error}"))
}

pub fn read_system_location() -> Result<SystemLocation, String> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|error| format!("初始化 Windows 定位服务失败：{error}"))?;
    let result = (|| {
        let locator =
            Geolocator::new().map_err(|error| format!("创建 Windows 定位服务失败：{error}"))?;
        let position = locator
            .GetGeopositionAsync()
            .map_err(|error| format!("启动系统位置读取失败：{error}"))?
            .get()
            .map_err(|error| format!("无法读取系统位置，请检查定位权限和服务：{error}"))?;
        location_from_position(position)
    })();
    unsafe { RoUninitialize() };
    result
}

fn location_from_position(position: Geoposition) -> Result<SystemLocation, String> {
    let point = position
        .Coordinate()
        .and_then(|coordinate| coordinate.Point())
        .map_err(|error| format!("系统没有提供经纬度位置：{error}"))?;
    let position = point
        .Position()
        .map_err(|error| format!("无法解析系统位置坐标：{error}"))?;
    let latitude = position.Latitude;
    let longitude = position.Longitude;
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        return Err("Windows 返回的位置坐标无效".into());
    }
    Ok(SystemLocation {
        latitude,
        longitude,
    })
}

fn post_location_result(hwnd: HWND, message: u32, result: Result<SystemLocation, String>) {
    let payload = Box::into_raw(Box::new(result));
    if unsafe {
        windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(hwnd),
            message,
            WPARAM(0),
            LPARAM(payload as isize),
        )
    }
    .is_err()
    {
        unsafe { drop(Box::from_raw(payload)) };
    }
}

pub fn fetch_weather(location: SystemLocation) -> Result<EnvironmentSnapshot, String> {
    let path = format!(
        "/v1/forecast?latitude={:.5}&longitude={:.5}&current=weather_code,is_day,cloud_cover,wind_speed_10m,wind_direction_10m&daily=sunrise,sunset&forecast_days=1&timezone=auto",
        location.latitude, location.longitude
    );
    let bytes = http_get(&path)?;
    let response: ForecastResponse =
        serde_json::from_slice(&bytes).map_err(|error| format!("天气数据格式无法识别：{error}"))?;
    let sunrise_minute = response
        .daily
        .sunrise
        .first()
        .and_then(|time| time.as_deref())
        .and_then(parse_local_minute);
    let sunset_minute = response
        .daily
        .sunset
        .first()
        .and_then(|time| time.as_deref())
        .and_then(parse_local_minute);
    Ok(EnvironmentSnapshot {
        location,
        sunrise_minute,
        sunset_minute,
        weather_code: response.current.weather_code,
        is_day: response.current.is_day != 0,
        cloud_cover: response.current.cloud_cover,
        wind_speed_kmh: response.current.wind_speed_10m,
        wind_direction: response.current.wind_direction_10m,
        updated_at: Instant::now(),
    })
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: CurrentWeather,
    daily: DailySunTimes,
}

#[derive(Deserialize)]
struct CurrentWeather {
    weather_code: i32,
    is_day: i32,
    cloud_cover: f32,
    wind_speed_10m: f32,
    wind_direction_10m: f32,
}

#[derive(Deserialize)]
struct DailySunTimes {
    sunrise: Vec<Option<String>>,
    sunset: Vec<Option<String>>,
}

fn parse_local_minute(value: &str) -> Option<u16> {
    let time = value.split('T').nth(1)?;
    let mut parts = time.split(':');
    let hour: u16 = parts.next()?.parse().ok()?;
    let minute: u16 = parts.next()?.parse().ok()?;
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}

struct HttpHandle(*mut c_void);

impl HttpHandle {
    fn new(raw: *mut c_void) -> Result<Self, String> {
        if raw.is_null() {
            Err("天气连接无法建立".into())
        } else {
            Ok(Self(raw))
        }
    }
}

impl Drop for HttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn http_get(path: &str) -> Result<Vec<u8>, String> {
    let session = HttpHandle::new(unsafe {
        WinHttpOpen(
            w!("ring-dock"),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })?;
    unsafe { WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 5000) }
        .map_err(|error| format!("设置天气连接超时失败：{error}"))?;
    let connection =
        HttpHandle::new(unsafe { WinHttpConnect(session.0, w!("api.open-meteo.com"), 443, 0) })?;
    let path_wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let request = HttpHandle::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path_wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    })?;
    unsafe { WinHttpSendRequest(request.0, None, None, 0, 0, 0) }
        .and_then(|()| unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) })
        .map_err(|error| format!("天气服务连接失败：{error}"))?;

    let mut body = Vec::new();
    loop {
        let mut available = 0u32;
        unsafe { WinHttpQueryDataAvailable(request.0, &mut available) }
            .map_err(|error| format!("读取天气数据失败：{error}"))?;
        if available == 0 {
            break;
        }
        if body.len().saturating_add(available as usize) > 64 * 1024 {
            return Err("天气服务返回内容超出限制".into());
        }
        let mut chunk = vec![0u8; available as usize];
        let mut read = 0u32;
        unsafe { WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), available, &mut read) }
            .map_err(|error| format!("读取天气数据失败：{error}"))?;
        body.extend_from_slice(&chunk[..read as usize]);
    }
    Ok(body)
}
