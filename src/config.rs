//! 配置（config.json）：象限数 / 透明度 / 时钟格式 / 各象限收纳条目
//! 首次运行自动生成默认配置；修改后右键圆环 →「重新加载配置」生效。
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 单个收纳条目：kind 取 program / file / folder / url
#[derive(Serialize, Deserialize, Clone)]
pub struct Item {
    pub name: String,
    pub kind: String,
    pub target: String,
}

/// 一个象限：label = 类型名（显示用），kind = 类型图标，items = 收纳内容
#[derive(Serialize, Deserialize, Clone)]
pub struct Quadrant {
    pub label: String,
    pub kind: String,
    /// 圆环分类图标；auto 根据分类名称选图标。
    #[serde(
        default = "default_category_icon",
        skip_serializing_if = "is_auto_category_icon"
    )]
    pub category_icon: String,
    pub items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// 象限数量（2..=8，默认 4）
    pub quadrant_count: usize,
    /// 圆环整体透明度 0.15..1.0
    pub opacity: f32,
    /// 中心时钟格式（strftime 风格，首版只支持 %H:%M / %I:%M %p / %H:%M:%S）
    pub clock_format: String,
    /// 图标尺寸（含名称格高约 +20）
    pub icon_size: f32,
    /// 面板条目图标风格；旧配置缺省为统一线条风格。
    #[serde(default)]
    pub icon_style: IconStyle,
    /// 圆环分类在图标、弧形文字和组合显示间切换。
    #[serde(default)]
    pub category_display_mode: CategoryDisplayMode,
    /// 面板图标横向间距
    pub item_gap: f32,
    /// 面板行距
    pub row_gap: f32,
    /// 面板内边距
    pub panel_padding: f32,
    /// 面板最大列数（超长换排）
    pub max_columns: usize,
    /// 面板最大高度占屏幕高的比例（超出内部滚动）
    pub max_height_ratio: f32,
    /// 已展开面板时点击其他象限：true=直接切换到该象限面板；false=先收起当前面板（再点才展开）
    #[serde(default = "default_true")]
    pub switch_panel_on_click: bool,
    /// 打开条目后自动收起面板
    #[serde(default = "default_true")]
    pub auto_collapse_after_open: bool,
    /// 登录 Windows 后自动启动；旧配置默认关闭。
    #[serde(default)]
    pub auto_start: bool,
    /// 圆环中心在主屏工作区中的比例；旧配置缺省为居中偏上。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dock_position: Option<DockPosition>,
    /// WinMemoryCleaner.exe 路径；为空时从 PATH 和常见安装位置自动查找。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub win_memory_cleaner_path: Option<String>,
    pub quadrants: Vec<Quadrant>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IconStyle {
    /// 当前的统一线条图标。
    #[default]
    Unified,
    /// Windows Shell 返回的原始图标。
    Original,
    /// Windows Shell 图标经过统一冷色调处理。
    Tinted,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CategoryDisplayMode {
    Icon,
    Text,
    #[default]
    Both,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DockPosition {
    pub x: f32,
    pub y: f32,
}

/// serde 缺省（旧配置文件兼容）
fn default_true() -> bool {
    true
}

fn default_category_icon() -> String {
    "auto".into()
}

fn is_auto_category_icon(icon: &String) -> bool {
    icon == "auto" || icon.is_empty()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            quadrant_count: 4,
            opacity: 0.85,
            clock_format: "%H:%M".into(),
            icon_size: 48.0,
            icon_style: IconStyle::default(),
            category_display_mode: CategoryDisplayMode::Both,
            item_gap: 12.0,
            row_gap: 16.0,
            panel_padding: 16.0,
            max_columns: 5,
            max_height_ratio: 0.72,
            switch_panel_on_click: true,
            auto_collapse_after_open: true,
            auto_start: false,
            dock_position: None,
            win_memory_cleaner_path: None,
            quadrants: vec![
                Quadrant {
                    label: "程序".into(),
                    kind: "program".into(),
                    category_icon: "auto".into(),
                    items: vec![
                        Item {
                            name: "记事本".into(),
                            kind: "program".into(),
                            target: "notepad.exe".into(),
                        },
                        Item {
                            name: "画图".into(),
                            kind: "program".into(),
                            target: "mspaint.exe".into(),
                        },
                        Item {
                            name: "终端".into(),
                            kind: "program".into(),
                            target: "wt.exe".into(),
                        },
                        Item {
                            name: "计算器".into(),
                            kind: "program".into(),
                            target: "calc.exe".into(),
                        },
                        Item {
                            name: "资源管理器".into(),
                            kind: "program".into(),
                            target: "explorer.exe".into(),
                        },
                        Item {
                            name: "截图".into(),
                            kind: "program".into(),
                            target: "ms-screenclip:".into(),
                        },
                    ],
                },
                Quadrant {
                    label: "文件".into(),
                    kind: "file".into(),
                    category_icon: "auto".into(),
                    items: vec![Item {
                        name: "待办.txt".into(),
                        kind: "file".into(),
                        target: r"C:\Users\Public\Documents\todo.txt".into(),
                    }],
                },
                Quadrant {
                    label: "文件夹".into(),
                    kind: "folder".into(),
                    category_icon: "auto".into(),
                    items: vec![
                        Item {
                            name: "下载".into(),
                            kind: "folder".into(),
                            target: r"C:\Users\Public\Downloads".into(),
                        },
                        Item {
                            name: "文档".into(),
                            kind: "folder".into(),
                            target: r"C:\Users\Public\Documents".into(),
                        },
                    ],
                },
                Quadrant {
                    label: "网址".into(),
                    kind: "url".into(),
                    category_icon: "auto".into(),
                    items: vec![Item {
                        name: "Bing".into(),
                        kind: "url".into(),
                        target: "https://www.bing.com".into(),
                    }],
                },
            ],
        }
    }
}

impl Config {
    /// 配置文件路径：exe 同目录 config.json
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("RING_DOCK_CONFIG") {
            return PathBuf::from(p);
        }
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("config.json")))
            .unwrap_or_else(|| PathBuf::from("config.json"))
    }

    /// 加载（不存在则写默认）；解析失败回退默认并保留原文件
    pub fn load() -> (Self, String) {
        let path = Self::path();
        if !path.exists() {
            let cfg = Self::default();
            let note = match cfg.save() {
                Ok(()) => format!("已生成默认配置：{}", path.display()),
                Err(e) => format!("默认配置无法保存：{e}"),
            };
            return (cfg, note);
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| {
                serde_json::from_str::<Self>(t.trim_start_matches('\u{feff}'))
                    .map_err(|e| e.to_string())
            }) {
            Ok(mut cfg) => {
                cfg.normalize();
                (cfg, String::new())
            }
            Err(e) => (Self::default(), format!("配置解析失败，已用默认值：{e}")),
        }
    }

    /// 写回配置文件（设置界面保存用）
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::path())
    }

    /// 同目录临时文件 + 原子替换，失败不破坏原配置。
    fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temp = path.with_file_name(format!(
            ".{}.{}.tmp",
            path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            drop(f);
            use std::os::windows::ffi::OsStrExt;
            let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(from.as_ptr()),
                    PCWSTR(to.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }

    /// 移动象限内的条目位置（拖拽排序）：from → to
    pub fn move_item(&mut self, qi: usize, from: usize, to: usize) -> bool {
        let q = match self.quadrants.get_mut(qi) {
            Some(q) => q,
            None => return false,
        };
        if from >= q.items.len() || to >= q.items.len() || from == to {
            return false;
        }
        let it = q.items.remove(from);
        q.items.insert(to, it);
        true
    }

    /// 删除象限内某个条目
    pub fn remove_item(&mut self, qi: usize, index: usize) -> bool {
        let q = match self.quadrants.get_mut(qi) {
            Some(q) => q,
            None => return false,
        };
        if index < q.items.len() {
            q.items.remove(index);
            return true;
        }
        false
    }

    /// 夹取合法范围
    fn normalize(&mut self) {
        self.dock_position = self
            .dock_position
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .map(|p| DockPosition {
                x: p.x.clamp(0.0, 1.0),
                y: p.y.clamp(0.0, 1.0),
            });
        self.quadrant_count = self.quadrant_count.clamp(2, 8);
        self.opacity = finite_clamp(self.opacity, 0.85, 0.15, 1.0);
        self.icon_size = finite_clamp(self.icon_size, 48.0, 28.0, 96.0);
        self.item_gap = finite_clamp(self.item_gap, 12.0, 0.0, 64.0);
        self.row_gap = finite_clamp(self.row_gap, 16.0, 0.0, 64.0);
        self.panel_padding = finite_clamp(self.panel_padding, 16.0, 8.0, 64.0);
        self.max_columns = self.max_columns.clamp(1, 8);
        self.max_height_ratio = finite_clamp(self.max_height_ratio, 0.72, 0.3, 0.95);
        if self.quadrants.is_empty() {
            self.quadrants = Self::default().quadrants;
        }
        // 象限数多于配置条目时循环复用
        let base_count = self.quadrants.len();
        while self.quadrants.len() < self.quadrant_count {
            let q = self.quadrants[self.quadrants.len() % base_count].clone();
            self.quadrants.push(q);
        }
        self.quadrants.truncate(self.quadrant_count);
    }

    /// 取当前时间字符串（首版时钟格式）
    pub fn clock_text(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // 本地时间（UTC + 东八区偏移由系统 TZ 环境变量推断，初版用 Win32 API 更准）
        let (h, m, s) = crate::sys::local_hms(now);
        match self.clock_format.as_str() {
            "%H:%M:%S" => format!("{h:02}:{m:02}:{s:02}"),
            "%I:%M %p" => {
                let ampm = if h < 12 { "AM" } else { "PM" };
                let h12 = match h % 12 {
                    0 => 12,
                    v => v,
                };
                format!("{h12:02}:{m:02} {ampm}")
            }
            _ => format!("{h:02}:{m:02}"),
        }
    }
}

fn finite_clamp(v: f32, default: f32, min: f32, max: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_and_invalid_position_are_safe() {
        let mut value = serde_json::to_value(Config::default()).unwrap();
        value.as_object_mut().unwrap().remove("dock_position");
        let old: Config = serde_json::from_value(value).unwrap();
        assert!(old.dock_position.is_none());
        let mut cfg = Config {
            dock_position: Some(DockPosition {
                x: f32::NAN,
                y: 0.5,
            }),
            ..Config::default()
        };
        cfg.normalize();
        assert!(cfg.dock_position.is_none());
        cfg.dock_position = Some(DockPosition { x: -1.0, y: 999.0 });
        cfg.normalize();
        assert_eq!(cfg.dock_position, Some(DockPosition { x: 0.0, y: 1.0 }));
    }

    #[test]
    fn invalid_geometry_is_normalized() {
        let mut cfg = Config {
            opacity: f32::NAN,
            icon_size: f32::INFINITY,
            item_gap: -999.0,
            row_gap: f32::NAN,
            panel_padding: -100.0,
            max_height_ratio: f32::NEG_INFINITY,
            quadrant_count: 99,
            ..Config::default()
        };
        cfg.normalize();
        assert_eq!(cfg.quadrant_count, 8);
        assert_eq!(cfg.quadrants.len(), 8);
        assert_eq!(cfg.quadrants[5].label, cfg.quadrants[1].label);
        assert_eq!(cfg.opacity, 0.85);
        assert_eq!(cfg.item_gap, 0.0);
        assert!(cfg.row_gap.is_finite() && cfg.panel_padding >= 8.0);
    }

    #[test]
    fn atomic_save_replaces_and_failure_preserves_original() {
        let dir = std::env::temp_dir().join(format!("orbit-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut cfg = Config::default();
        cfg.save_to(&path).unwrap();
        cfg.opacity = 0.9;
        cfg.save_to(&path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            serde_json::from_str::<Config>(&content).unwrap().opacity,
            0.9
        );
        let lock = path.with_file_name(format!(".config.json.{}.tmp", std::process::id()));
        std::fs::write(&lock, "occupied").unwrap();
        assert!(cfg.save_to(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        assert_eq!(std::fs::read_to_string(&lock).unwrap(), "occupied");
        std::fs::remove_file(&lock).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn move_and_remove_items() {
        let mut cfg = Config::default();
        let names: Vec<String> = cfg.quadrants[0]
            .items
            .iter()
            .map(|i| i.name.clone())
            .collect();
        // 0 → 2：插到第三位
        assert!(cfg.move_item(0, 0, 2));
        assert_eq!(cfg.quadrants[0].items[2].name, names[0]);
        assert_eq!(cfg.quadrants[0].items[0].name, names[1]);
        // 删除第 2 个（即刚移过去的）
        assert!(cfg.remove_item(0, 2));
        assert_eq!(cfg.quadrants[0].items.len(), names.len() - 1);
        // 越界安全
        assert!(!cfg.move_item(0, 0, 99));
        assert!(!cfg.remove_item(0, 99));
    }
}
