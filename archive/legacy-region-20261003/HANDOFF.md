# HANDOFF —— ring-dock 项目交接档案

> **2026-10-03 半透明结论更正（先调研/PoC，未改正式 UI）**：下面 §四-B-10/11、§四-G-30/32b 以及 §七中“桌面子窗口不能用 alpha”“半透明只有 alpha=255 才可命中”的绝对结论已被本机反例推翻。`WS_EX_LAYERED + UpdateLayeredWindow` 在实际 `SysListView32`/`SHELLDLL_DefView` 桌面宿主下合成正常，动态底层测试正常；66 色值断言和 18 个 `WindowFromPoint` 命中查询全部通过（不是实际鼠标 E2E）。直接创建 layered child 需兼容 manifest；popup→SetParent 两种 manifest 档均可工作，旧失败的完整根因未定。Progman 对照不可见，本轮无结论。证据与边界见 **`reports/transparency/REPORT.md`**；可重跑 **`tools/transparency/run.ps1`**。另隔离复现 `textrgn.rs` 未设 `SetBkMode(TRANSPARENT)` 导致路径成为“字框减字形”的问题。本轮保留旧记载作为历史，不再把其当作设计禁令。

> 供新会话/协作者快速接手。包含：项目现状、架构、**经实战验证的可靠经验（踩坑档案）**、
> 回归测试手册、已知边界。更新时间：2026-10（盘缘抗锯齿 + 圆心锚点面板 + 中心白玻璃后）。

## 一、项目是什么

**ring-dock**：Windows 桌面圆环收纳挂件（Rust + 纯 Win32/Direct2D，零第三方 UI 框架）。
半透明圆环固定在屏幕中心，按象限收纳程序/文件/文件夹/网址；点击象限展开玻璃面板点图标打开。

**两个核心刚需（已解决）**：
1. **固定显示在桌面上**：不遮挡任何应用窗口、不受 Win+D /「最小化所有」影响；
2. **好点、不卡、不闪**：点击响应 30~50ms，无闪烁无残影。

## 二、架构与模块地图

```
ring-dock/
├── crates/deskpin/        ★ 零业务可复用 crate：「固定显示在桌面上」全部能力
│   └── src/lib.rs         Pinner（挂载/自愈）/ find_desktop_host（降级链）/
│                          place_workarea / repaint_desktop_area / wide / DPI
├── src/
│   ├── main.rs            窗口创建、消息循环、App 状态、挂载自愈接线、托盘/设置接线
│   ├── hit.rs             ★ 点击命中测试（纯函数 + 5 个单测），与绘制同源
│   ├── render.rs          Direct2D 绘制（圆盘/细弧/线性图标/中心白玻璃时钟/面板，双缓冲）
│   ├── bg.rs              ★ 抗锯齿底图：抓屏 + 形状边界带「向外采样重建」（盘缘平滑的根基）
│   ├── icons.rs           矢量图标（program/file/folder/url，圆头线条）
│   ├── config.rs          config.json 读写（serde，旧文件自动补默认字段）
│   ├── tray.rs            托盘图标（Explorer 重启自重建）
│   ├── settings_ui.rs     设置窗口（纯 Win32 控件）
│   ├── open.rs            ShellExecute 打开条目
│   └── sys.rs             本地时间/日期、deskpin::wide re-export
├── tools/                 ★ 回归测试脚本（PowerShell，见§六）
├── assets/ring.ico        exe 图标（build.rs/winres 嵌入）
└── README.md              用户文档（含调研报告与方案对比）
```

依赖：`windows 0.61`、`serde/serde_json`、`windows-numerics`、`winres`（build）。
构建：`cargo build --release`；单测：`cargo test`。

## 三、当前功能清单

- 圆环按象限收纳条目（2~8 象限可配），点击弧段展开玻璃面板，面板内线性图标+名称网格
- **长按面板 0.55s 进入编辑态**：条目出现删除角标（可删）、可拖动换位、点空白退出编辑
- **拖拽收纳**：文件/快捷方式拖进面板自动入册（OLE IDropTarget），自动推断类型并写回 config.json
- **展开面板以圆心为锚点**（近角固定在圆心，朝点击象限向外展开；覆盖展开象限的弧段与中心时钟）
- **盘缘抗锯齿**：形状边缘与真实桌面混色（见 §四-G），图标完整不被盘缘切掉
- **中心白玻璃圆**：纯白+透明度的圆形底 + 时间/日期排版在上
- 命中行为：弧段=展开/收起/切换；弧缝、面板空白、中心=收起；面板开着点其他弧段=切换或先收起（可配）
- 滚轮内滚、右键菜单（设置/重载配置/打开配置/退出）、托盘图标（同菜单，正常退出入口）
- 设置窗口：`switch_panel_on_click`（切换 or 先收起）、`auto_collapse_after_open`（打开后自动收面板）
- 中心时钟 + 日期行（`%H:%M` / `%H:%M:%S` / `%I:%M %p`）
- 桌面固定显示 + 挂载自愈（Explorer 重启自动重建重挂）
- config.json 全量可配（象限/条目/透明度/布局尺寸等）

## 四、★ 实战验证过的可靠经验（踩坑档案）

### A. 桌面固定显示（deskpin 的存在依据）

1. **唯一同时满足"不遮挡应用 + 抗 Win+D"的方式 = SetParent 挂进桌面窗口树**。
   `WS_EX_TOPMOST` 两个需求都不满足（盖住应用、Win+D 照样最小化）；
   Rainmeter 走的是 `EVENT_SYSTEM_FOREGROUND` 钩子+定时器压 Z 序，官方承认偶发失效。
2. **挂载点降级链**：`SysListView32`（图标之上）→ `SHELLDLL_DefView` → Progman 子级 WorkerW
   （图标之下，Win11 24H2 起）→ 顶层 WorkerW（Win10 壁纸层，`0x052C` 触发创建）→ Progman。
3. **桌面子窗口不参与顶层窗口的最小化/显示桌面**（Win+D 只动 Progman 之外的普通窗口）——结构性免疫。
4. **WS_POPUP→WS_CHILD 转换后 SetParent**；SetParent 返回值与"此前无父"难区分，**必须 GetParent 回读验证**，失败还原样式。
5. **父链必须加 WS_CLIPCHILDREN**，否则图标层重绘会把挂件擦掉。
6. **Explorer 重启/换壁纸会连带销毁子窗口**（跨进程父子的宿命）→ `WM_DESTROY` 里重建窗口重挂
   （区分 quitting 标志），+ 定时器（~10s）`Pinner::maintain` 自愈。
7. **桌面子窗口收不到 WM_DISPLAYCHANGE** → 定时器对比工作区几何变化。
8. Win11 若关闭系统属性→性能→"动画控件和元素"，Explorer **不创建 WorkerW**（仅影响图标之下档位）。
9. `0x052C` 私有消息是触发 WorkerW 创建的惯用法（Win10）；Win11 24H2 层级收进 Progman。

### B. 透明与鼠标命中（本轮最大的坑，代价最高）

10. **`WS_EX_LAYERED` 窗口的命中测试与像素透明度挂钩**（MSDN："based on the shape **and transparency**"）：
    - 色键透明（`LWA_COLORKEY`）区 **完全点不中**（点击穿到下层）——视觉上"细线+全透明底"时
      用户必须精准点中线条像素，体验极差（实测复现）；
    - 统一透明度（`LWA_ALPHA`，210/255）**也大幅劣化**（实测几乎点不中）；
    - layered 窗口里**唯一可靠命中的是 alpha=255 实心像素**。
11. **"半透明 + 随便点"在单窗口无解**，只有三条路（全部趟过）：
    - **不透明玻璃块**（普通窗口 + SetWindowRgn，透明感用绘制模拟）→ 当前方案，命中 100%；
    - **抓屏混色假透明**（普通窗口预合成桌面快照）→ 好点，但要维护快照（卡顿/闪烁/时机坑）；
    - **双层窗口**（透明视觉层 + alpha≈0 的命中垫层转发鼠标）→ 两者兼得但复杂，已废弃。
12. **普通窗口的命中 = 纯 Rgn**，与绘制内容无关 → "好看不好点"和"好点"可以解耦：
    视觉画什么无所谓，SetWindowRgn 的形状说了算。Rgn 内全域可点是交互手感的关键。
13. `CreateWindowExW` 直接创建 `WS_CHILD`（父=layered 窗口）会失败且 GetLastError=0；
    创建为顶层再 `SetParent` +样式转换可以（若将来需要子窗口挂 layered 父）。

### C. 绘制、闪烁与性能

14. **立即模式（DCRenderTarget）下 `Clear(色键色)` 的中间帧用户看得见**（"闪紫"根因）。
    底色要么是最终观感的一部分，要么=透明（普通窗口做不到"Clear 成透明"）。
    类背景刷同理：`WM_ERASEBKGND` 的擦除帧也会闪，背景刷要与最终底色一致或吞掉擦除。
15. **点击路径上不能有重活**：全屏抓取 + CPU 模糊（百万像素上采样）曾把点击拖到 100~300ms
    且 `SW_HIDE` 抓屏造成"圆环消失一下"。现在点击路径只有 SetWindowRgn + 重绘一帧 = 30~50ms
    （首次 98ms 是 D2D/字体一次性初始化）。
16. **blur（盒式模糊）滑窗实现的经典 bug**：初始窗口和必须是 `clamp(k-r)`（边缘重复语义），
    写成 `min(k, w-1)` 会让滑窗"减去和里没有的值"→ u32 下溢（debug panic / release 结果算花）。
17. `RedrawWindow` 带 `RDW_UPDATENOW` 是**跨进程同步重绘**，与 Explorer 互相等待有阻塞/死锁风险；
    只做"请桌面重绘"用异步 flags，且放在低频路径（定时器），别放点击路径。

### D. 残影（SetWindowRgn 缩小的暴露区）

18. **Rgn 缩小后暴露的像素没人自动擦**（Explorer 桌面平时静态）→ 收起面板会留残影。
    经验：**普通窗口**需要"交还桌面重绘"（`repaint_desktop_area`，RedrawWindow 异步）；
    实测收起后与基线像素差 0.278（≈完美还原）。**layered 窗口走 DWM 合成一般不残影**。
19. 若必须精确还原：**先铺回干净快照再缩 Rgn**（Rgn 外绘制会被裁剪，顺序不能反）；
    快照抓取时**自己的绘制还在屏上** → 快照含自己 → 铺回去等于没消失，必须 hide→抓→show。

### E. 测试方法论（tools/ 脚本的血泪）

20. **验证鼠标命中必须用真实鼠标**（`SetCursorPos + mouse_event`，走系统 hit-test）；
    `SendMessage` 直达窗口过程，绕过 hit-test，会给出假阳性。
21. **像素对比判据要免疫干扰**（动态壁纸、别的桌面挂件、被窗口遮挡、历史残影）：
    残影的特征判据 = "收起后 ≈ 展开时"（Diff(after, open) 小），不受背景变化影响。
22. **Win+D 是开关式**：必须按需触发（Covered 检测）且失败路径要还原，否则把用户窗口搞得最小化/错乱。
23. **Rgn 面积（GetRegionData）比 bbox 可靠**：小面板（1 图标）完全落在圆盘 bbox 内，bbox 看不出展开。
24. `PrintWindow` 对跨进程桌面子窗口不可靠（回退抓屏幕合成）；`GetDC+BitBlt` 抓的是
    "屏幕共享缓冲的该区域 + 自身绘制"的混合，**被遮挡时会抓到遮挡窗口的内容**。
25. 测试污染管理：失败路径必须复原配置（`switch_panel_on_click` 等）、收起面板、还原窗口，
    否则下一轮测试的前提被破坏（真实发生过）。

### F. 工具链坑

26. **cargo 的 mtime 指纹**：文件在同一秒内被修改可能不触发重编（"Finished 0.93s"假构建）；
    症状是"改了代码行为没变"，`touch` 文件或等 1 秒重编。
27. **PowerShell**：`[Cls]::Method` 不带括号只是打印方法定义（静默不执行！）；静态方法传参要括号；
    `ConvertFrom-Json` 对象不能直接赋新属性（用 `Add-Member`）；Add-Type 指定
    `-ReferencedAssemblies` 会覆盖默认引用集（System.Text 等丢失）。
28. 退出时窗口/托盘立即消失，但**进程收尾（D2D/COM/GDI 清理）3~20s**（随负载波动）——
    Windows D2D 程序常见现象，非 bug；测试判定要放宽（脚本已按 30s 轮询）。

### G. 抗锯齿与「真透明」（本轮，代价同样很高）

29. **普通窗口 + SetWindowRgn 的形状边界是硬切的**（GDI 区域无 alpha）：盘缘锯齿的唯一根源。
    弧线/图标/文字由 D2D 画，本来就有 AA；锯齿全在 Rgn 边界上（45° 斜边 1px 台阶）。
30. **WS_EX_LAYERED + UpdateLayeredWindow（逐像素 alpha）在桌面树里不合成**（`layer_probe.ps1` 实测）：
    SetParent 进 SysListView32 后内容完全不可见（但 alpha=255 实心区真实鼠标命中正常、
    alpha=0 穿透）；顶层分层窗口一切正常（边缘平滑、好点）。即「真透明/逐像素 alpha」
    这条正统路在桌面子窗口下不可用，别再试。不转 WS_CHILD 只 SetParent 也挂不上（回读=0）。
31. **SetWindowRgn 会裁剪分层窗口的显示**（探针实测）：想用 Rgn 放大命中区来保边缘平滑，行不通。
32. D2D 抗锯齿常量反直觉：**`D2D1_ANTIALIAS_MODE_PER_PRIMITIVE = 0`、`ALIASED = 1`**；
    把 1 当「逐基元」会**关掉**抗锯齿（本轮真实踩过：盘缘 1px 硬跳，看起来像“AA 没生效”）。
32b. **色键透明（LWA_COLORKEY）在桌面树里同样不合成**（`colorkey_probe.ps1` 实测）：
    顶层一切正常（真透明+精确色+好点）；SetParent 进桌面树后内容同样不可见（命中却正常）。
    即：**只要挂进桌面树，WS_EX_LAYERED 的任何透明机制（逐像素 α / 色键）都不会被合成**；
    想要真透明只剩一条路：**把窗口形状（Rgn）挖洞**——洞里没有窗口像素，壁纸自然透出。
32c. 真透明中心的时钟文字 = **文字轮廓构造窗口区域**（`textrgn.rs`：BeginPath/DrawTextW/EndPath/
    PathToRegion，WidenPath 加宽出光晕区）：文字画在光晕（深色）上，窗口形状=字形∪光晕；
    文字与形状用同一 GDI 字体/矩形 → 精确对齐。文字变化时必须重建区域（`rebuild_clock_rgn`）。
32d. **`RegisterDragDrop` 必须先 `OleInitialize`（仅 CoInitializeEx 不够）**：注册会**静默失败**，
    表现=拖到窗口上鼠标显示 🚫。注意 🚫 有两个含义：①没找到拖放目标 ②目标返回 DROPEFFECT_NONE——
    光看图标分不清，**一定要把注册结果与 DragEnter 写日志**（`drop_log.txt`）才能定位。
32e. 自建 OLE 拖源（DoDragDrop + 自实现 IDataObject）在 PowerShell/.NET10 上有 marshaling 崩溃，
    合成拖拽 E2E 不可靠；拖放回归用手拖（真实 shell 拖拽）+ 逻辑单测（`drop.rs` 的 3 个 test）相结合。
33. 最终方案「**抓屏底图 + 形状边界带向外采样重建 + D2D AA**」（`src/bg.rs`）：
    - 形状边缘 AA 混色需要「真实桌面」当底图 → 抓屏（BitBlt→32bpp DIB，alpha 要手动置 255）；
    - 但抓屏含自己上一帧 → 直接当底会把边缘混色逐帧再混一遍（5~10 帧后边缘变硬，实测）；
      必须把每个形状边界带（±2.5px）换成「向外 4px 处」的采样——那里在 Rgn 之外，
      永远是真桌面，不被自身绘制污染；
    - **采样距离必须 > Rgn 外边距**（4 > 2），否则采到自己上一帧的内容（缓慢漂移）。
34. **DCRenderTarget 立即模式的中间帧可见**（§四-C-14）：重绘节奏提到秒级后必须双缓冲
    （内存 DC + 单次 BitBlt），否则每 2s 闪一帧 Clear 底色。
35. 面板改为圆心锚点后，小面板完全落在圆盘内 → **GetWindowRgn 面积法无法再观测展开/收起**；
    改用窗口标题 `ring-dock#expanded=N[#edit]` 作测试观测点（无标题栏，用户不可见）。
35b. **内容变化后的刷新不要走 `set_expanded`**（它会清编辑态/重置滚动）：用 `refresh_panel()`
    （保留编辑态与滚动，只更新标题/命中形状/重绘）。删除、排序、拖入后都用它。
35c. **点击动作放在 WM_LBUTTONUP 触发**（不是按下）：否则长按会先误触发一次点击。
    长按判定以**按下点**为准（不是当前光标）：按住时轻微移动不取消，自动化测试也不依赖真鼠标。
36. 抓屏类测试脚本的 PowerShell 坑：`New-Object Type(2*$a, 2*$b)` 里逗号优先级高于乘法，
    会被解析成数组乘法（`op_Multiply on Object[]`）；先算成变量再传（§四-F-27 同类）。

## 五、关键实现索引（改代码先看这些）

| 要改什么 | 去哪里 |
|---|---|
| 桌面挂载/自愈/工作区 | `crates/deskpin/src/lib.rs`（`Pinner`、`find_desktop_host`） |
| 点击行为/命中判定 | `src/hit.rs`（纯函数+单测）、`src/main.rs::on_click` |
| 长按/编辑态/拖动排序 | `src/main.rs::on_mouse_down/up/move、on_hold`；角标命中在 `hit.rs` |
| 拖拽收纳（OLE IDropTarget） | `src/drop.rs`（落点判定/条目推断/写回配置） |
| 窗口形状（盘环带挖洞/文字区域/面板） | `src/main.rs::update_hit_rgn` + `src/textrgn.rs` |
| 视觉（盘/弧/图标/面板/悬浮时钟） | `src/render.rs`；图标 `src/icons.rs` |
| 盘缘抗锯齿（底图混色） | `src/bg.rs`（抓屏 + 边界带重建）；接线在 `render.rs::refresh_bg` |
| 拖拽收纳（OLE IDropTarget） | `src/drop.rs`（落点判定/条目推断/写回配置） |
| 圆盘半径/图标边距 | `render.rs::RingGeom::r_disc`（= r_mid+27，必须包住图标外接半径） |
| 中心透明洞半径 | `render.rs::RingGeom::r_chip` |
| 面板布局（圆心锚点，绘制与命中共用） | `render.rs::PanelLayout::item_cell`（**唯一来源，勿另写公式**） |
| 托盘/设置/配置 | `src/tray.rs`、`src/settings_ui.rs`、`src/config.rs` |
| 颜色/透明观感 | `src/render.rs` 顶部 `INK/ACCENT` 常量 |

## 六、回归测试手册（tools/）

| 脚本 | 验证 | 备注 |
|---|---|---|
| `cargo test` | 命中几何单测（弧/缝/中心/面板格、跨区误触发回归） | 快，先跑 |
| `smoke_interact.ps1` | 13 项交互（点击/切换两档/面板空白收起/设置/退出） | Rgn 观测，不干扰桌面 |
| `smoke_embed.ps1` | 桌面挂载层级 + Win+D 免疫 | debug 构建；会按 Win+D |
| `smoke_erase.ps1` | 残影（截图像素对比） | 自动按需 Win+D 显示桌面再还原 |
| `hit_check.ps1` | **真实鼠标**命中验证（点弧段内空白） | 验命中必跑 |
| `vis_check.ps1` | 视觉验收（窗口 DC 截图，不怕遮挡） | 改绘制后跑 |
| `preview.ps1` | **视觉预览**（真实屏幕截图 收起/展开 + 盘缘 AA 量化） | 改视觉后跑，截图直接给用户 |
| `layer_probe.ps1` | 分层窗口可行性探针（逐像素 alpha × 桌面树 × Rgn，见 §四-G） | 动透明方案前必跑 |
| `colorkey_probe.ps1` | 色键透明可行性探针（顶层/桌面树对比） | 动透明方案前必跑 |
| `wallpaper_probe.ps1` | 壁纸层抓取探针（PrintWindow / 屏幕合成对比） | 动底图方案前跑 |
| `edit_test.ps1` | 编辑态 E2E（长按进入→删除→拖动换位→退出） | 编辑功能改动后跑 |
| `drag_drop_test.ps1` + `drag_src.ps1` | 拖拽收纳 E2E（合成 OLE 拖源；**注：合成拖源当前进不了拖拽循环**，见 §七） | 拖放改动后跑/手测 |

注意事项见 §四-E/F；测试会短暂 Win+D 显示桌面（结束自动还原），用户全屏游戏时慎跑屏幕类脚本。

## 七、已知边界 / 未做

- 只支持主屏（工作区矩形；DPI 已 Per-Monitor V2）
- 时钟格式仅三种；面板滚轮依赖系统"悬停滚动"设置
- 拖拽排序、开机自启未做
- **中心是“形状挖洞”的真透明**（壁纸透出）；时钟文字是字形+光晕的窗口形状，
  光晕外缘是硬切 1px（小元素，视觉上像描边/阴影）；文字区不可再叠真半透明（见 §四-G-32b）
- **盘缘外 2px 边距显示「向外 4px 采样」的近似壁纸色**：静态壁纸下肉眼无感；
  动态壁纸下边缘色与真实壁纸有细微差异（每 ~2s 重绘跟踪）
- 空闲时每 ~2s 重绘一次（跟踪动态壁纸），CPU ≈0.1~0.2%
- **拖拽收纳**：已接 OLE IDropTarget（注册需 OleInitialize，见 §四-G-32d）；落点判定/条目推断/写回配置有单测；
  E2E 靠**手拖**验证；诊断日志在 `target/release/drop_log.txt`（注册结果 + DragEnter/Drop/提取）
- 视觉细节（弧粗细、色值、字号、光晕宽度）都是常量级可调，等用户审美反馈迭代

## 八、新会话快速上手

1. 读本文件 + `README.md`（调研报告与方案对比在 README）；
2. `cargo build --release` → `cargo test` → 跑 `tools/` 脚本确认环境；
3. 改动命中/布局相关必须同步 `hit.rs` 单测；改视觉用 `target/vis_check.ps1`（窗口 DC 截图）验收；
4. **动透明/窗口样式前先读 §四-B**（layered 命中陷阱，别再踩）；改视觉用 `tools/vis_check.ps1` 截图验收。
