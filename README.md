# ring-dock —— Windows 桌面圆环收纳工具（Rust 初版）

> **接手/开新会话请先读 [`HANDOFF.md`](HANDOFF.md)**（项目现状、架构、经验坑案、回归测试手册）。
> 本文偏功能与调研；HANDOFF 偏工程交接。

半透明圆环常驻屏幕中心（类似游戏表情盘），按象限收纳桌面图标；点击象限展开毛玻璃面板后点图标打开，平时点击穿透、不打扰。

## 视觉与实现对照（线稿 → 代码）

| 线稿口径 | 实现 |
|---|---|
| 覆盖层叠在壁纸上，只存引用路径 | 桌面树嵌入的覆盖层；`config.json` 保存目标引用，不移动/复制真实文件 |
| 未点击 = 点击穿透 | 单窗口 + `SetWindowRgn` 只保留「圆盘 ∪ 面板」，形状外天然穿透、不拦截 |
| 视觉：深夜玻璃 + 冰蓝强调（全矢量） | 渐变弧段（圆头描边）+ 端点帽 + 玻璃徽章 + 中心时钟/日期；玻璃面板卡（模糊底 + 墨色压暗 + 顶部反光 + 边缘高光）+ 图标卡渐变 + 矢量图标 |
| 圆环均分象限（默认 4，可配） | `quadrant_count`（2~8），弧段圆帽描边、段间留缝 |
| 每象限一个类型图标 | program / file / folder / url 四类**矢量图标**（Direct2D 几何，无图片素材） |
| 中心时钟（内容可配） | DirectWrite 文本，`clock_format`：%H:%M / %H:%M:%S / %I:%M %p |
| 展开：该象限弧段消失让位 | 绘制时跳过展开象限弧段；面板一角紧贴圆心朝象限展开 |
| 渐变毛玻璃面板（圆心侧朦胧→边缘清晰） | 展开瞬间抓桌面快照 → CPU 3 轮盒式模糊缓存 → 径向渐变透明度混合（圆心侧模糊浓、边缘归零露出实时桌面） |
| 无边框/无标题栏，纯图标排布 | 无任何窗口装饰，只有柔光背景 + 图标格（白圆角块+矢量图形+名称） |
| 同类同一横排、超长换排 | `max_columns` 列换排 |
| 默认无滚动条，内容超多才浮现 | `scroll_max > 0` 时才绘制滚动条；滚轮/拖动内滚 |
| 面板不超出屏幕 | `max_height_ratio` 上限 + 防御性夹取 |
| 低资源 | 消息驱动零轮询；时钟文本无变化不重绘；模糊只在展开时算一次 |

## 固定显示在桌面上：调研与实现

需求：常驻桌面、**不遮挡任何应用窗口**、**不受 Win+D /「最小化所有」影响**。

| 方案 | 代表 | 不遮挡 | 抗最小化 | 备注 |
|---|---|---|---|---|
| `WS_EX_TOPMOST` 顶层置顶 | 早期本项目 | ✗ 盖住所有应用 | ✗ Win+D 照样最小化 | 两个需求都不满足，已弃用 |
| Z 序动态维护 | [Rainmeter `AlwaysOnTop=-2`](https://github.com/rainmeter/rainmeter/issues/339) | ✓ | ✓ | 需 `EVENT_SYSTEM_FOREGROUND` 钩子 + 定时器持续压 Z 序，官方承认偶发乱序失效 |
| **SetParent 挂进桌面窗口树** | Lively Wallpaper、AutoIt 桌面贴图、Electron 桌面挂件方案 | ✓ | ✓ | 作为桌面子窗口：不属于顶层窗口，Win+D/最小化所有只动 Progman 之外的普通窗口；零轮询 |

本项目采用**桌面窗口树嵌入**，能力已抽成独立 crate **`crates/deskpin`**（零业务、可直接复用），
挂载点降级链（`deskpin::find_desktop_host`）：

```
SysListView32（图标之上）→ SHELLDLL_DefView → Progman 子级 WorkerW（图标之下，Win11 24H2 起）
→ 顶层 WorkerW（Win10 壁纸层，0x052C 触发创建）→ Progman（兜底）
```

配套的健壮性处理：

- Explorer 重启 / 切换壁纸会连带销毁桌面子窗口 → `WM_DESTROY` 里**重建窗口并重挂**（不退出）+ 定时器低频校验挂载点
- 父链加 `WS_CLIPCHILDREN`，防止图标层重绘擦掉挂件
- 桌面子窗口收不到 `WM_DISPLAYCHANGE` → 定时器对比工作区几何变化后重新摆放
- 挂载失败退回顶层并把 Z 序压到 `HWND_BOTTOM`（仍满足不遮挡；此档位才可能被 Win+D 带走，定时器持续重试嵌入）
- **自擦（防残影）**：桌面子窗口的暴露区没人自动擦（Explorer 平时静态不重绘），
  `SetWindowRgn` 缩小（收起面板）后会留残影。配套机制（`deskpin::blit_snapshot` + `repaint_desktop_area`）：
  **先把即将暴露的区域铺回干净桌面快照（必须在缩 Rgn 之前，Rgn 外绘制会被裁剪）**，
  再异步请桌面树重绘换回真身。窗口整体挪动同理。

实测（Windows 11）：挂载层级 `RingDockVisual ← SysListView32 ← SHELLDLL_DefView ← Progman`，
Win+D 后普通窗口全部最小化而挂件保持可见未最小化。

## 模块地图（业务与能力分离）

| 模块 | 职责 | 业务耦合 |
|---|---|---|
| `crates/deskpin` | 「固定显示在桌面上」全部能力：桌面树嵌入 / 挂载自愈（`Pinner`）/ 工作区摆放 / DPI | **无**（可整目录拷走复用） |
| `src/hit.rs` | 点击命中测试（纯函数 + 单测），与绘制几何严格一一对应 | 几何即业务，但逻辑独立 |
| `src/tray.rs` | 托盘图标（Explorer 重启自重建） | 菜单命令即业务 |
| `src/settings_ui.rs` | 设置窗口（纯 Win32 控件） | 设置项即业务 |
| `src/render.rs / blur.rs / icons.rs` | Direct2D 绘制 / 模糊 / 矢量图标 | 业务 |
| `src/sys.rs` | 仅剩业务用小工具（屏幕快照 / 本地时间） | 业务 |

接入 deskpin 只需三步（详见其文档）：`enable_dpi_awareness()` → `Pinner::attach(hwnd)` →
定时器里 `Pinner::maintain(hwnd)`；并在 `WM_DESTROY`（非用户退出）时重建窗口再 `attach`。

## 构建与运行

```powershell
cargo build --release
.\target\release\ring-dock.exe
```

首次运行在 exe 同目录生成 `config.json`。仓库为 cargo workspace（根 = 业务，`crates/deskpin` = 可复用能力）。

## 交互

- **左键点击象限弧段**：展开该象限面板（**展开时圆环整体消失**，只剩玻璃面板）；再点同一弧段 = 收起；已展开时点击其他弧段 = 切换或先收起（设置可配）
- **左键点击面板图标**：打开该项（程序/文件/文件夹/网址）；打开后是否自动收起面板可配
- **点击空白处**（弧缝 / 面板空白 / 中心时钟）：收起面板（保证“回得去”）
- **滚轮**：面板内容超多时内滚
- **右键圆环 / 托盘图标**：菜单（设置… / 重新加载配置 / 打开配置文件 / 退出）
- **托盘图标**：正常退出入口（左/右键均弹菜单），Explorer 重启后自动重建

命中判定与绘制几何严格一致（`src/hit.rs`，带单元测试）：点弧缝不误触发、点 A 弧不会触发 B 象限。

绘制要点（防闪烁/残影）：

- 每帧第一笔 = 铺清晰桌面快照（**不用色键色 Clear**——品红中间帧就是“闪紫”根因；类背景刷也置空）
- 快照 = **纯桌面**（抓屏前短暂隐藏自身，毫秒级；blur 后置计算不占隐藏时长）；
  展开态把圆环区铺回纯快照 = 视觉上圆环消失；每 ~60s 周期刷新快照跟进桌面变化

## config.json

```jsonc
{
  "quadrant_count": 4,        // 象限数 2~8
  "opacity": 0.85,            // 圆环整体透明度
  "clock_format": "%H:%M",    // 中心时钟格式
  "icon_size": 48,            // 面板图标尺寸
  "item_gap": 12, "row_gap": 16, "panel_padding": 16,
  "max_columns": 5,           // 每排最多列数（超长换排）
  "max_height_ratio": 0.72,   // 面板最大高度（屏高比例，超出内滚）
  "switch_panel_on_click": true,       // 展开时点其他象限：true=直接切换，false=先收起再点才展开
  "auto_collapse_after_open": true,    // 打开条目后自动收起面板
  "quadrants": [
    {
      "label": "程序", "kind": "program",
      "items": [
        { "name": "记事本", "kind": "program", "target": "notepad.exe" },
        { "name": "Bing",  "kind": "url",     "target": "https://www.bing.com" }
      ]
    }
  ]
}
```

`kind` 取 `program` / `file` / `folder` / `url`；`target` 为可执行名/路径/目录/网址。

## 回归测试

- `cargo test`：命中测试单元测试（弧段/弧缝/中心/面板格，含跨区域误触发回归）
- `tools/smoke_embed.ps1`：桌面挂载 + Win+D 免疫（debug 构建）
- `tools/smoke_interact.ps1`：交互行为回归（模拟点击 + GetRegionData 精确观测命中区域：弧缝不误触发/展开收起/切换两档/面板空白收起/设置窗口/退出）
- `tools/smoke_erase.ps1`：残影回归（截图像素对比：收起/切换后面板内容必须消失；判据不受动态壁纸与遮挡干扰，自动按需 Win+D）

## 已知边界（初版）

- 只支持主屏（多屏/缩放下用工作区矩形，DPI 已设 Per-Monitor V2）
- 面板展开瞬间的桌面快照为背景模糊源（展开后桌面变化不追帧）
- 时钟格式仅支持上述三种
- Explorer 重启/换壁纸会连带销毁嵌入子窗口（已自动重建重挂，建议手动验证一次）
- Win11 若在「系统属性 → 性能选项」关闭了「动画控件和元素」，Explorer 不创建 WorkerW（仅影响图标之下档位）
- 面板滚轮依赖系统「悬停时滚动非活动窗口」设置（Win10/11 默认开启）
- 退出时窗口与托盘立即消失，但进程收尾（D2D/COM/GDI 清理）实测需 3~20s（随机器负载波动）才从进程表消失
- 拖拽排序 / 开机自启未做（后续迭代）
