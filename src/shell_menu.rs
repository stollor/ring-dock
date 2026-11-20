//! 面板图标的右键菜单。
//! 能定位到真实文件或文件夹时，弹出和资源管理器相同的外壳菜单。
//! 网址、协议，以及磁盘上已经不存在的路径，没有外壳对象，退回「打开 / 复制」。

use std::cell::Cell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use windows::core::{Interface, PCSTR, PCWSTR};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Storage::FileSystem::{GetFullPathNameW, SearchPathW};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VIRTUAL_KEY, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IContextMenu, IContextMenu2, IContextMenu3, ILFree, IShellFolder, SHBindToParent,
    SHParseDisplayName, CMF_CANRENAME, CMF_EXPLORE, CMF_EXTENDEDVERBS, CMF_ITEMMENU, CMF_NORMAL,
    CMIC_MASK_CONTROL_DOWN, CMIC_MASK_PTINVOKE, CMIC_MASK_SHIFT_DOWN, CMINVOKECOMMANDINFO,
    CMINVOKECOMMANDINFOEX,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetCursorPos, GetMenuItemCount, LoadCursorW, RegisterClassW, SetForegroundWindow, ShowWindow,
    TrackPopupMenu, HMENU, IDC_ARROW, MF_STRING, SW_SHOW, SW_SHOWNORMAL, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_DRAWITEM, WM_INITMENU, WM_INITMENUPOPUP, WM_MEASUREITEM,
    WM_MENUCHAR, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// `lpVerbW` 生效。windows crate 0.61 没有导出这个掩码。
const CMIC_MASK_UNICODE: u32 = 0x0000_4000;
const CMD_FIRST: u32 = 1;
const CMD_LAST: u32 = 0x7FFF;
const FALLBACK_OPEN: u32 = 1;
const FALLBACK_COPY: u32 = 2;
const CLASS_NAME: &str = "RingDockShellMenu";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// 菜单已关闭。外壳命令已经由系统执行，或用户取消了。
    Dismissed,
    /// 没有外壳对象时，用户选了「打开」。
    Open,
}

struct ComInit {
    owned: bool,
}

impl ComInit {
    fn enter() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // S_OK 才是本次初始化。界面线程已经 OleInitialize 时不能再 CoUninitialize。
        Self { owned: hr.0 == 0 }
    }
}

impl Drop for ComInit {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

struct Pidl(*mut ITEMIDLIST);

impl Drop for Pidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ILFree(Some(self.0.cast())) };
        }
    }
}

struct Popup(HMENU);

impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

struct Owner(HWND);

impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

struct ShellObject {
    _pidl: Pidl,
    _folder: IShellFolder,
    context: IContextMenu,
    popup: Popup,
}

struct Forward {
    menu3: Option<IContextMenu3>,
    menu2: Option<IContextMenu2>,
}

thread_local! {
    static FORWARD: Cell<*const Forward> = Cell::new(std::ptr::null());
}

struct ClearForward;

impl Drop for ClearForward {
    fn drop(&mut self) {
        FORWARD.with(|slot| slot.set(std::ptr::null()));
    }
}

pub fn show(target: &str) -> Choice {
    let _com = ComInit::enter();
    let expanded = expand_env(strip_quotes(target.trim()));
    if let Some(path) = resolve_existing(&expanded) {
        match show_shell(&path) {
            Ok(()) => return Choice::Dismissed,
            Err(error) => eprintln!("[ring-dock] 外壳菜单失败：{error}"),
        }
    } else if !expanded.is_empty() && show_shell(&expanded).is_ok() {
        return Choice::Dismissed;
    }
    show_fallback(&expanded)
}

fn show_shell(path: &str) -> Result<(), String> {
    let prepared = prepare_shell_item(path)?;
    let point = cursor();
    let owner = menu_owner(point)?;
    let object = prepared.into_menu(owner.0)?;
    let forward = Forward {
        menu3: object.context.cast().ok(),
        menu2: object.context.cast().ok(),
    };
    FORWARD.with(|slot| slot.set(&forward));
    let _clear = ClearForward;
    let command = track(owner.0, object.popup.0, point);
    FORWARD.with(|slot| slot.set(std::ptr::null()));
    if (CMD_FIRST..=CMD_LAST).contains(&command) {
        if let Err(error) = invoke(&object.context, owner.0, command - CMD_FIRST, point) {
            eprintln!("[ring-dock] 执行外壳命令失败：{error}");
        }
    }
    Ok(())
}

struct PreparedItem {
    pidl: Pidl,
    child: *mut ITEMIDLIST,
    folder: IShellFolder,
}

fn prepare_shell_item(path: &str) -> Result<PreparedItem, String> {
    unsafe {
        let wide = crate::sys::wide(path);
        let mut pidl = std::ptr::null_mut();
        SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)
            .map_err(|error| format!("无法解析「{path}」：{error}"))?;
        let pidl = Pidl(pidl);
        let mut child = std::ptr::null_mut();
        let folder: IShellFolder = SHBindToParent(pidl.0, Some(&mut child))
            .map_err(|error| format!("无法绑定父级：{error}"))?;
        if child.is_null() {
            return Err("外壳子项为空".into());
        }
        Ok(PreparedItem {
            pidl,
            child,
            folder,
        })
    }
}

#[cfg(test)]
fn load_shell_object(path: &str, owner: HWND) -> Result<ShellObject, String> {
    prepare_shell_item(path)?.into_menu(owner)
}

impl PreparedItem {
    fn into_menu(self, owner: HWND) -> Result<ShellObject, String> {
        unsafe {
            let items = [self.child as *const ITEMIDLIST];
            let context: IContextMenu = self
                .folder
                .GetUIObjectOf(owner, &items, None)
                .map_err(|error| format!("无法取得外壳菜单：{error}"))?;
            let popup = Popup(CreatePopupMenu().map_err(|error| error.to_string())?);
            let mut flags = CMF_NORMAL | CMF_EXPLORE | CMF_CANRENAME | CMF_ITEMMENU;
            if key_down(VK_SHIFT) {
                flags |= CMF_EXTENDEDVERBS;
            }
            let hr = context.QueryContextMenu(popup.0, 0, CMD_FIRST, CMD_LAST, flags);
            let count = GetMenuItemCount(Some(popup.0));
            if count <= 0 {
                return Err(format!("外壳菜单为空（{hr:?}）"));
            }
            Ok(ShellObject {
                _pidl: self.pidl,
                _folder: self.folder,
                context,
                popup,
            })
        }
    }
}

fn invoke(context: &IContextMenu, owner: HWND, offset: u32, point: POINT) -> Result<(), String> {
    let mut mask = CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE;
    if key_down(VK_SHIFT) {
        mask |= CMIC_MASK_SHIFT_DOWN;
    }
    if key_down(VK_CONTROL) {
        mask |= CMIC_MASK_CONTROL_DOWN;
    }
    let mut info = CMINVOKECOMMANDINFOEX {
        cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        fMask: mask,
        hwnd: owner,
        lpVerb: PCSTR(offset as usize as *const u8),
        lpVerbW: PCWSTR(offset as usize as *const u16),
        nShow: SW_SHOWNORMAL.0,
        ptInvoke: point,
        ..Default::default()
    };
    unsafe {
        context
            .InvokeCommand(&mut info as *mut CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO)
            .map_err(|error| error.to_string())
    }
}

fn show_fallback(target: &str) -> Choice {
    let Ok(owner) = menu_owner(cursor()) else {
        return Choice::Dismissed;
    };
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return Choice::Dismissed;
    };
    let popup = Popup(menu);
    let open = crate::sys::wide("打开");
    let copy = crate::sys::wide("复制");
    unsafe {
        let _ = AppendMenuW(
            popup.0,
            MF_STRING,
            FALLBACK_OPEN as usize,
            PCWSTR(open.as_ptr()),
        );
        let _ = AppendMenuW(
            popup.0,
            MF_STRING,
            FALLBACK_COPY as usize,
            PCWSTR(copy.as_ptr()),
        );
    }
    let point = cursor();
    let command = track(owner.0, popup.0, point);
    if command == FALLBACK_COPY && !target.is_empty() {
        if let Err(error) = copy_text(target) {
            eprintln!("[ring-dock] 复制失败：{error}");
        }
    }
    if command == FALLBACK_OPEN {
        Choice::Open
    } else {
        Choice::Dismissed
    }
}

fn track(owner: HWND, menu: HMENU, point: POINT) -> u32 {
    unsafe {
        let _ = ShowWindow(owner, SW_SHOW);
        let _ = SetForegroundWindow(owner);
        TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            point.x,
            point.y,
            None::<i32>,
            owner,
            None::<*const RECT>,
        )
        .0 as u32
    }
}

fn menu_owner(point: POINT) -> Result<Owner, String> {
    ensure_class()?;
    let class = crate::sys::wide(CLASS_NAME);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            WS_POPUP,
            point.x,
            point.y,
            1,
            1,
            None,
            None,
            Some(deskpin::module_handle()),
            None,
        )
    }
    .map_err(|error| format!("无法创建菜单窗口：{error}"))?;
    Ok(Owner(hwnd))
}

fn ensure_class() -> Result<(), String> {
    static ONCE: Once = Once::new();
    static OK: AtomicBool = AtomicBool::new(false);
    ONCE.call_once(|| {
        OK.store(register_class().is_ok(), Ordering::Release);
    });
    if OK.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err("外壳菜单窗口类注册失败".into())
    }
}

fn register_class() -> Result<(), String> {
    let class = crate::sys::wide(CLASS_NAME);
    unsafe {
        let window = WNDCLASSW {
            lpfnWndProc: Some(owner_proc),
            hInstance: deskpin::module_handle(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        if RegisterClassW(&window) == 0 {
            let error = windows::Win32::Foundation::GetLastError();
            if error != ERROR_CLASS_ALREADY_EXISTS {
                return Err(format!("RegisterClassW 失败：{error:?}"));
            }
        }
    }
    Ok(())
}

unsafe extern "system" fn owner_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if matches!(
        msg,
        WM_INITMENU | WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        let forward = FORWARD.with(|slot| slot.get());
        if !forward.is_null() {
            let forward = unsafe { &*forward };
            if let Some(menu3) = &forward.menu3 {
                let mut result = LRESULT(0);
                if unsafe {
                    menu3
                        .HandleMenuMsg2(msg, w, l, Some(std::ptr::addr_of_mut!(result)))
                        .is_ok()
                } {
                    return if msg == WM_MENUCHAR {
                        result
                    } else {
                        LRESULT(0)
                    };
                }
            } else if let Some(menu2) = &forward.menu2 {
                if unsafe { menu2.HandleMenuMsg(msg, w, l).is_ok() } {
                    return LRESULT(0);
                }
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

fn copy_text(text: &str) -> Result<(), String> {
    let units = crate::sys::wide(text);
    unsafe {
        let memory =
            GlobalAlloc(GMEM_MOVEABLE, units.len() * 2).map_err(|error| error.to_string())?;
        let locked = GlobalLock(memory);
        if locked.is_null() {
            let _ = windows::Win32::Foundation::GlobalFree(Some(memory));
            return Err("无法锁定剪贴板内存".into());
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), locked.cast(), units.len());
        let _ = GlobalUnlock(memory);
        if OpenClipboard(None).is_err() {
            let _ = windows::Win32::Foundation::GlobalFree(Some(memory));
            return Err("无法打开剪贴板".into());
        }
        let _ = EmptyClipboard();
        let copied = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0)));
        let _ = CloseClipboard();
        if copied.is_err() {
            let _ = windows::Win32::Foundation::GlobalFree(Some(memory));
            return Err("无法写入剪贴板".into());
        }
    }
    Ok(())
}

fn cursor() -> POINT {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    point
}

fn key_down(key: VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(i32::from(key.0)) < 0 }
}

fn strip_quotes(text: &str) -> &str {
    text.trim().trim_matches('"')
}

fn expand_env(text: &str) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push('%');
            out.push_str(after);
            return out;
        };
        let name = &after[..end];
        if !name.is_empty() {
            if let Ok(value) = std::env::var(name) {
                out.push_str(&value);
                rest = &after[end + 1..];
                continue;
            }
        }
        out.push('%');
        out.push_str(name);
        out.push('%');
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn is_non_file(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    if lower.contains("://") {
        return true;
    }
    match lower.find(':') {
        Some(index) if index > 1 => {
            let rest = &lower[index + 1..];
            !rest.starts_with('\\') && !rest.starts_with('/')
        }
        _ => false,
    }
}

fn resolve_existing(target: &str) -> Option<String> {
    if target.is_empty() || is_non_file(target) {
        return None;
    }
    if let Some(full) = existing_full(target) {
        return Some(full);
    }
    if target.contains('\\') || target.contains('/') {
        return None;
    }
    search_path(target).or_else(|| app_paths(target))
}

fn existing_full(path: &str) -> Option<String> {
    let full = full_path(path).unwrap_or_else(|| path.to_string());
    Path::new(&full).exists().then_some(full)
}

fn full_path(path: &str) -> Option<String> {
    let wide = crate::sys::wide(path);
    let mut buf = vec![0u16; 32768];
    let length = unsafe { GetFullPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buf), None) };
    if length == 0 || length as usize >= buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..length as usize]))
}

fn search_path(name: &str) -> Option<String> {
    let wide = crate::sys::wide(name);
    let mut buf = vec![0u16; 32768];
    let length = unsafe {
        SearchPathW(
            PCWSTR::null(),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            Some(&mut buf),
            None,
        )
    };
    if length == 0 || length as usize >= buf.len() {
        return None;
    }
    let found = String::from_utf16_lossy(&buf[..length as usize]);
    Path::new(&found).exists().then_some(found)
}

fn app_paths(name: &str) -> Option<String> {
    let file_name = Path::new(name).file_name()?.to_str()?;
    app_path_key(file_name).or_else(|| {
        if Path::new(file_name).extension().is_some() {
            None
        } else {
            app_path_key(&format!("{file_name}.exe"))
        }
    })
}

fn app_path_key(name: &str) -> Option<String> {
    let subkey = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{name}");
    read_app_path(HKEY_CURRENT_USER, &subkey).or_else(|| read_app_path(HKEY_LOCAL_MACHINE, &subkey))
}

fn read_app_path(root: HKEY, subkey: &str) -> Option<String> {
    let wide = crate::sys::wide(subkey);
    let mut buf = vec![0u16; 1024];
    let mut bytes = (buf.len() * 2) as u32;
    let error = unsafe {
        RegGetValueW(
            root,
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if error.0 != 0 || bytes < 2 {
        return None;
    }
    let chars = (bytes as usize / 2).saturating_sub(1);
    command_path(&String::from_utf16_lossy(&buf[..chars.min(buf.len())]))
}

fn command_path(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let path = if let Some(rest) = raw.strip_prefix('"') {
        rest.split('"').next().unwrap_or(rest)
    } else {
        raw.split_whitespace().next().unwrap_or(raw)
    };
    Path::new(path).exists().then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_program_on_path() {
        let path = resolve_existing("notepad.exe").expect("notepad.exe");
        assert!(path.to_ascii_lowercase().ends_with(r"\notepad.exe"));
        assert!(Path::new(&path).is_file());
    }

    #[test]
    fn skips_url_protocol_and_missing_file() {
        assert!(resolve_existing("https://www.bing.com").is_none());
        assert!(resolve_existing("ms-screenclip:").is_none());
        assert!(resolve_existing(r"C:\Users\Public\Documents\ring-dock-missing.txt").is_none());
    }

    #[test]
    fn resolves_existing_directory() {
        let docs = r"C:\Users\Public\Documents";
        if Path::new(docs).is_dir() {
            let got = resolve_existing(docs).expect("documents");
            assert!(Path::new(&got).is_dir());
        }
    }

    #[test]
    fn expands_environment_variables() {
        let name = "RING_DOCK_SHELL_MENU_TEST";
        std::env::set_var(name, "notepad.exe");
        assert_eq!(expand_env(&format!("%{name}%")), "notepad.exe");
        std::env::remove_var(name);
        assert_eq!(
            expand_env("%RING_DOCK_SHELL_MENU_ABSENT%"),
            "%RING_DOCK_SHELL_MENU_ABSENT%"
        );
    }

    #[test]
    fn shell_menu_lists_file_and_directory_commands() {
        let _com = ComInit::enter();
        let notepad = resolve_existing("notepad.exe").expect("notepad.exe");
        let file_count = menu_count(&notepad).expect("notepad menu");
        assert!(file_count > 2, "notepad menu count {file_count}");

        let dir = std::env::temp_dir();
        let dir_count = menu_count(&dir.to_string_lossy()).expect("directory menu");
        assert!(dir_count > 2, "directory menu count {dir_count}");

        let file = dir.join(format!("ring-dock-shell-menu-{}.txt", std::process::id()));
        std::fs::write(&file, "ring-dock").unwrap();
        let count = menu_count(&file.to_string_lossy()).expect("file menu");
        let _ = std::fs::remove_file(&file);
        assert!(count > 2, "file menu count {count}");
    }

    fn menu_count(path: &str) -> Result<i32, String> {
        let object = load_shell_object(path, HWND::default())?;
        Ok(unsafe { GetMenuItemCount(Some(object.popup.0)) })
    }
}
