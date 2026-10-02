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
    pub quadrants: Vec<Quadrant>,
}

/// serde 缺省（旧配置文件兼容）
fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            quadrant_count: 4,
            opacity: 0.85,
            clock_format: "%H:%M".into(),
            icon_size: 48.0,
            item_gap: 12.0,
            row_gap: 16.0,
            panel_padding: 16.0,
            max_columns: 5,
            max_height_ratio: 0.72,
            switch_panel_on_click: true,
            auto_collapse_after_open: true,
            quadrants: vec![
                Quadrant {
                    label: "程序".into(),
                    kind: "program".into(),
                    items: vec![
                        Item { name: "记事本".into(), kind: "program".into(), target: "notepad.exe".into() },
                        Item { name: "画图".into(), kind: "program".into(), target: "mspaint.exe".into() },
                        Item { name: "终端".into(), kind: "program".into(), target: "wt.exe".into() },
                        Item { name: "计算器".into(), kind: "program".into(), target: "calc.exe".into() },
                        Item { name: "资源管理器".into(), kind: "program".into(), target: "explorer.exe".into() },
                        Item { name: "截图".into(), kind: "program".into(), target: "ms-screenclip:".into() },
                    ],
                },
                Quadrant {
                    label: "文件".into(),
                    kind: "file".into(),
                    items: vec![
                        Item { name: "待办.txt".into(), kind: "file".into(), target: r"C:\Users\Public\Documents\todo.txt".into() },
                    ],
                },
                Quadrant {
                    label: "文件夹".into(),
                    kind: "folder".into(),
                    items: vec![
                        Item { name: "下载".into(), kind: "folder".into(), target: r"C:\Users\Public\Downloads".into() },
                        Item { name: "文档".into(), kind: "folder".into(), target: r"C:\Users\Public\Documents".into() },
                    ],
                },
                Quadrant {
                    label: "网址".into(),
                    kind: "url".into(),
                    items: vec![
                        Item { name: "Bing".into(), kind: "url".into(), target: "https://www.bing.com".into() },
                    ],
                },
            ],
        }
    }
}

impl Config {
    /// 配置文件路径：exe 同目录 config.json
    pub fn path() -> PathBuf {
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
            if let Ok(text) = serde_json::to_string_pretty(&cfg) {
                let _ = std::fs::write(&path, text);
            }
            return (cfg, format!("已生成默认配置：{}", path.display()));
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Self>(&t).map_err(|e| e.to_string()))
        {
            Ok(mut cfg) => {
                cfg.normalize();
                (cfg, String::new())
            }
            Err(e) => (Self::default(), format!("配置解析失败，已用默认值：{e}")),
        }
    }

    /// 写回配置文件（设置界面保存用）
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())
    }

    /// 夹取合法范围
    fn normalize(&mut self) {
        self.quadrant_count = self.quadrant_count.clamp(2, 8);
        self.opacity = self.opacity.clamp(0.15, 1.0);
        self.icon_size = self.icon_size.clamp(28.0, 96.0);
        self.max_columns = self.max_columns.clamp(1, 8);
        self.max_height_ratio = self.max_height_ratio.clamp(0.3, 0.95);
        if self.quadrants.is_empty() {
            self.quadrants = Self::default().quadrants;
        }
        // 象限数多于配置条目时循环复用
        while self.quadrants.len() < self.quadrant_count {
            let q = self.quadrants[self.quadrants.len() % self.quadrants.len()].clone();
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
