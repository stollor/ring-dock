# HANDOFF —— ring-dock 项目交接档案

> 供新会话/协作者快速接手。包含：项目现状、架构、**经实战验证的可靠经验（踩坑档案）**、
> 回归测试手册、已知边界。更新时间：2026-10（视觉极简化 + 真透明方案定案后）。

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
│   ├── render.rs          Direct2D 绘制（极简玻璃风：细弧/线性图标/时钟/面板）
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
- **展开时仅隐藏被点击象限的弧段**（其余圆环/时钟照常），面板从圆环外缘展开
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

## 五、关键实现索引（改代码先看这些）

| 要改什么 | 去哪里 |
|---|---|
| 桌面挂载/自愈/工作区 | `crates/deskpin/src/lib.rs`（`Pinner`、`find_desktop_host`） |
| 点击行为/命中判定 | `src/hit.rs`（纯函数+单测）、`src/main.rs::on_click` |
| 命中区形状 | `src/main.rs::update_hit_rgn`（与视觉形状同步） |
| 视觉（弧/图标/时钟/面板） | `src/render.rs`；图标 `src/icons.rs` |
| 面板布局（绘制与命中共用） | `src/render.rs::PanelLayout::item_cell`（**唯一来源，勿另写公式**） |
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

注意事项见 §四-E/F；测试会短暂 Win+D 显示桌面（结束自动还原），用户全屏游戏时慎跑屏幕类脚本。

## 七、已知边界 / 未做

- 只支持主屏（工作区矩形；DPI 已 Per-Monitor V2）
- 时钟格式仅三种；面板滚轮依赖系统"悬停滚动"设置
- 拖拽排序、开机自启未做
- 玻璃为不透明深色块（真透明会牺牲命中，见 §四-B）——可调颜色/加渐变模拟透光
- 视觉细节（弧粗细、色值、字号）都是常量级可调，等用户审美反馈迭代

## 八、新会话快速上手

1. 读本文件 + `README.md`（调研报告与方案对比在 README）；
2. `cargo build --release` → `cargo test` → 跑 `tools/` 脚本确认环境；
3. 改动命中/布局相关必须同步 `hit.rs` 单测；改视觉用 `target/vis_check.ps1`（窗口 DC 截图）验收；
4. **动透明/窗口样式前先读 §四-B**（layered 命中陷阱，别再踩）；改视觉用 `tools/vis_check.ps1` 截图验收。
