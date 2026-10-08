//! 拖拽收纳：把文件 / 快捷方式 / .url 拖到**展开的面板**上 → 自动加入该象限并写回 config.json
//!
//! 接入点是 OLE `IDropTarget`（系统拖拽的标准协议）：Explorer 的文件拖拽提供 `CF_HDROP`。
//! 只在「面板展开且落点在面板内」时接受拖放（否则 DROPEFFECT_NONE，拖拽指针显示禁止）。
use windows::core::{implement, Ref, Result as WResult};
use windows::Win32::Foundation::POINTL;
use windows::Win32::System::Com::{IDataObject, FORMATETC};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

/// 拖放目标：持有 App 指针（消息循环单线程，拖放回调也在这条线程上）
#[implement(IDropTarget)]
pub struct DropTarget {
    pub(crate) app: *mut crate::App,
    pub(crate) accepts_files: std::cell::Cell<bool>,
}
/// 显式指定 RING_DOCK_DROP_LOG 路径才记录诊断，不污染正式安装目录。
pub fn dlog(s: &str) {
    let Some(path) = std::env::var_os("RING_DOCK_DROP_LOG") else {
        return;
    };
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{s}")
        });
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WResult<()> {
        dlog(&format!(
            "DragEnter pt=({}, {}) data={}",
            pt.x,
            pt.y,
            pdataobj.is_some()
        ));
        let fmt = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: 1,
            lindex: -1,
            tymed: 1,
        };
        self.accepts_files.set(
            pdataobj
                .as_ref()
                .is_some_and(|d| unsafe { d.QueryGetData(&fmt) }.is_ok()),
        );
        unsafe { self.set_effect(pt, pdweffect) }
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WResult<()> {
        unsafe { self.set_effect(pt, pdweffect) }
    }

    fn DragLeave(&self) -> WResult<()> {
        self.accepts_files.set(false);
        unsafe {
            (*self.app).drag_leave();
        }
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WResult<()> {
        unsafe {
            let app = &mut *self.app;
            if !self.accepts_files.get()
                || ((*pdweffect).0 & DROPEFFECT_COPY.0) == 0
                || !app.drop_target_ok(pt.x as f32, pt.y as f32)
            {
                dlog(&format!("Drop 拒绝（不在面板内）pt=({}, {})", pt.x, pt.y));
                *pdweffect = DROPEFFECT_NONE;
                app.drag_leave();
                return Ok(());
            }
            *pdweffect = DROPEFFECT_COPY;
            let paths = collect_hdrop(pdataobj.as_ref());
            dlog(&format!("Drop 接受 paths={paths:?}"));
            app.on_drop_files(&paths);
            app.drag_leave();
            self.accepts_files.set(false);
            if paths.is_empty() {
                *pdweffect = DROPEFFECT_NONE;
            }
        }
        Ok(())
    }
}

impl DropTarget_Impl {
    /// 评估落点：面板展开且在面板内 → 允许复制
    unsafe fn set_effect(&self, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> WResult<()> {
        let app = &mut *self.app;
        *pdweffect = if self.accepts_files.get()
            && ((*pdweffect).0 & DROPEFFECT_COPY.0) != 0
            && app.drag_over(pt.x as f32, pt.y as f32)
        {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        };
        Ok(())
    }
}

/// 从数据对象取 CF_HDROP 文件列表
unsafe fn collect_hdrop(data: Option<&IDataObject>) -> Vec<String> {
    let mut out = Vec::new();
    let data = match data {
        Some(d) => d,
        None => return out,
    };
    let fmt = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: 1, // DVASPECT_CONTENT
        lindex: -1,
        tymed: 1, // TYMED_HGLOBAL
    };
    let mut medium = match data.GetData(&fmt as *const _) {
        Ok(m) => m,
        Err(e) => {
            dlog(&format!("collect_hdrop GetData 失败：{e}"));
            return out;
        }
    };
    let hdrop = HDROP(medium.u.hGlobal.0);
    let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
    dlog(&format!("collect_hdrop：{count} 个文件"));
    for i in 0..count {
        let mut buf = vec![0u16; DragQueryFileW(hdrop, i, None) as usize + 1];
        let n = DragQueryFileW(hdrop, i, Some(&mut buf));
        if n > 0 {
            out.push(String::from_utf16_lossy(&buf[..n as usize]));
        }
    }
    windows::Win32::System::Ole::ReleaseStgMedium(&mut medium);
    out
}

/// 落点判定（纯逻辑）：面板展开且落点（屏幕坐标）落在面板内
#[allow(clippy::too_many_arguments)] // Shared layout/geometry inputs, kept explicit.
pub fn point_in_panel(
    cfg: &crate::config::Config,
    geom: &crate::render::RingGeom,
    screen_w: f32,
    screen_h: f32,
    expanded: Option<usize>,
    origin: (i32, i32),
    sx: f32,
    sy: f32,
) -> bool {
    let qi = match expanded {
        Some(q) => q,
        None => return false,
    };
    let items = cfg.quadrants.get(qi).map(|q| q.items.len()).unwrap_or(0);
    let lay = crate::render::layout_panel(cfg, items, geom, screen_w, screen_h, qi);
    let x = sx - origin.0 as f32;
    let y = sy - origin.1 as f32;
    lay.contains(x, y)
}

/// 把拖入的路径转成条目并追加到象限；返回新增数量（纯逻辑，可单测）
pub fn add_items(cfg: &mut crate::config::Config, qi: usize, paths: &[String]) -> usize {
    let mut added = 0usize;
    for p in paths {
        if p.is_empty() {
            continue;
        }
        let item = crate::config::Item {
            name: display_name(p),
            kind: infer_kind(p).to_string(),
            target: p.clone(),
        };
        if let Some(q) = cfg.quadrants.get_mut(qi) {
            if !q
                .items
                .iter()
                .any(|existing| existing.target.eq_ignore_ascii_case(p))
            {
                q.items.push(item);
                added += 1;
            }
        }
    }
    added
}

/// 由路径推断条目类型（与 config 的 kind 取值一致）
pub fn infer_kind(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".url") {
        "url"
    } else if lower.ends_with(".lnk")
        || lower.ends_with(".exe")
        || lower.ends_with(".bat")
        || lower.ends_with(".cmd")
    {
        "program"
    } else if std::path::Path::new(path).is_dir() {
        "folder"
    } else {
        "file"
    }
}

/// 显示名 = 文件名去扩展名
pub fn display_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_paths_and_invalid_quadrant_are_ignored() {
        let mut cfg = crate::config::Config::default();
        assert_eq!(
            add_items(
                &mut cfg,
                1,
                &[
                    r"C:\Temp\report.txt".into(),
                    r"c:\temp\REPORT.txt".into(),
                    "".into()
                ]
            ),
            1
        );
        assert_eq!(add_items(&mut cfg, 99, &["test.txt".into()]), 0);
    }

    #[test]
    fn kind_and_name() {
        assert_eq!(infer_kind(r"C:\\a\\b.lnk"), "program");
        assert_eq!(infer_kind(r"C:\\a\\b.url"), "url");
        assert_eq!(infer_kind(r"C:\\a\\b.txt"), "file");
        assert_eq!(display_name(r"C:\\a\\b.txt"), "b");
        assert_eq!(display_name(r"C:\\a\\记事本.lnk"), "记事本");
    }

    #[test]
    fn add_into_quadrant() {
        let mut cfg = crate::config::Config::default();
        let before = cfg.quadrants[1].items.len();
        let n = add_items(
            &mut cfg,
            1,
            &[r"C:\\Temp\\todo.txt".into(), r"C:\\Temp\\x.lnk".into()],
        );
        assert_eq!(n, 2);
        assert_eq!(cfg.quadrants[1].items.len(), before + 2);
        assert_eq!(cfg.quadrants[1].items[before].name, "todo");
        assert_eq!(cfg.quadrants[1].items[before + 1].kind, "program");
    }

    #[test]
    fn drop_point_rules() {
        let cfg = crate::config::Config::default();
        let geom = crate::render::RingGeom {
            cx: 500.0,
            cy: 400.0,
            r_mid: 117.0,
            stroke: 26.0,
            n: 4,
        };
        // 未展开：任何点都不收
        assert!(!point_in_panel(
            &cfg,
            &geom,
            1000.0,
            800.0,
            None,
            (0, 0),
            600.0,
            300.0
        ));
        // 展开 0（右上、圆心锚点）：面板内接受、面板外拒绝
        let items = cfg.quadrants[0].items.len();
        let lay = crate::render::layout_panel(&cfg, items, &geom, 1000.0, 800.0, 0);
        let inside_x = lay.x + lay.w / 2.0;
        let inside_y = lay.y + lay.h / 2.0;
        assert!(point_in_panel(
            &cfg,
            &geom,
            1000.0,
            800.0,
            Some(0),
            (0, 0),
            inside_x,
            inside_y
        ));
        assert!(!point_in_panel(
            &cfg,
            &geom,
            1000.0,
            800.0,
            Some(0),
            (0, 0),
            20.0,
            20.0
        ));
        // 窗口原点偏移：屏幕坐标 = 客户区坐标 + origin
        assert!(point_in_panel(
            &cfg,
            &geom,
            1000.0,
            800.0,
            Some(0),
            (100, 50),
            inside_x + 100.0,
            inside_y + 50.0
        ));
    }
}
