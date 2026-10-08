# Ring Dock · Orbit Glass

Windows 桌面圆环收纳工具。当前界面使用真正的逐像素透明：暗色玻璃、四色细线光轨、清晰时钟与下置收藏面板。

## 当前渲染与设计

- `Direct2D / DirectWrite → 32-bit 预乘 BGRA DIB → UpdateLayeredWindow(ULW_ALPHA)`。
- 桌面宿主由独立 `deskpin` 库管理；生产窗口嵌入桌面，**不置顶盖住应用**。
- 背景半透明，文字主体不随玻璃透明度一起变淡；DirectWrite 使用灰度抗锯齿，避免透明表面的 ClearType 彩边。
- 不抓壁纸、不缓存屏幕充当背景、不使用色键、文字区域挖洞或 `SetWindowRgn`。
- 当前是透明玻璃视觉，**不是 Acrylic / 实时背景模糊**。不要把半透明和毛玻璃模糊混为一谈。
- 圆环保持可用；下方独立面板包含整理/完成、关闭、滚动条、空状态、拖放/启动错误提示。
- 展开有 160ms 淡入，结束停止动画定时器。没有常驻高频动画。
- 绘制缓冲与透明窗口只覆盖圆环及展开面板的边界，收起即释放大缓冲。可见时环境动画最高约 15 FPS，被前台窗口完全遮挡时停止动画定时器；160ms 展开动画独立运行。原图标按可见行加载，缓存上限 128 项，切换分类、样式或删除收藏会淘汰旧资源；提取结果队列上限 32，同一时间只有一批提取任务。

## 操作

| 操作 | 结果 |
|---|---|
| 点分类弧片 | 展开；再点相同分类收起 |
| 展开时点其他分类 | 按设置切换分类，或先收起 |
| 点中心 / 面板 × | 整理态退出整理；面板展开时中心收起；圆环收起时点中心清理旧临时文件并请求低优先级内存整理；× 直接收起 |
| 点面板空白 | 保持打开，不误关闭 |
| 点收藏 | 正常模式打开目标；失败保留面板并提示 |
| 点「整理」或长按面板 550ms | 进入整理；拖动换位，点 − 移除收藏（不删除原文件） |
| 按住鼠标中键（滚轮）拖动 | 从圆环或面板任意可见位置移动整个挂件；松开保存位置，重启恢复 |
| 滚轮 | 超长内容内部滚动；中键移动过程中暂不滚动 |
| 拖文件经过分类 400ms | 自动展开该分类；移到面板松开，加入并保存 |
| 重复拖入同一路径 | 忽略重复项 |
| 右键圆环 / 托盘 | 设置、重载、打开配置、退出 |

完全透明区域交给下层桌面处理，所以**不承诺在任意外部位置点击都能关闭面板**。窗口不抢键盘焦点，当前不把全局 Esc 当作已完成能力。

拖放是收藏路径，不搬移原文件。接受 `CF_HDROP` 且源允许 COPY；不接受仅允许 MOVE 的源或纯文本源。`.lnk` / `.url` 使用 ShellExecute 打开。

## 配置

默认读取 exe 同目录 `config.json`；字段兼容旧配置。保存采用同目录临时文件、flush/sync 和 Windows 原子替换。保存错误在面板提示，不静默伪装成功。

支持 2–8 分类、透明度、三种时钟格式、图标大小、间距、列数、滚动高度、切换分类及启动后收起偏好。设置中的“圆环分类显示”可选只显示图标、只显示沿圆弧排布的文字，或同时显示图标和文字；“各分类图标”可单独选择自动语义图标、对话气泡、代码、AI 星芒、播放按钮、程序窗口、文件、文件夹或网址图标。旧配置默认使用图标和文字、按分类名称自动选图标。设置中的“面板条目图标样式”可选统一默认线条图标、Windows 原图标，或经过低饱和冷色调处理的原图标。异常数值会归一化。`dock_position: {"x": 0.5, "y": 0.4}` 保存主屏工作区中的相对中心位置；旧配置无该字段时使用默认位置。边缘保留圆环与阴影，靠近底部时面板改在上方展开。当前仍限主屏工作区，不宣称跨屏拖动。

收起圆环后点击中心会清理当前用户 `%TEMP%` 下超过 24 小时的文件，跳过正在使用的文件和重解析点，并显示释放的磁盘空间。内存整理调用 WinMemoryCleaner 的 `/StandbyListLowPriority` 命令。启动 ring-dock 时会请求一次 UAC，以启动仅处理内存整理的后台助手；助手随 ring-dock 运行，点击清理时不再重复弹窗，主界面保持普通权限。默认从 ring-dock 同目录、`%LOCALAPPDATA%\RingDock`、PATH、常见 WinGet/Scoop/Program Files 位置查找，也可在 `config.json` 设置 `win_memory_cleaner_path` 指向 `WinMemoryCleaner.exe`。当前用户级便携版存放在 `%LOCALAPPDATA%\RingDock\WinMemoryCleaner.exe`，来源为[官方 3.0.8 发布页](https://github.com/IgorMundstein/WinMemoryCleaner/releases/tag/3.0.8)，GPL-3.0。未找到工具时仍会完成临时文件清理并提示内存整理未执行。

诊断仅显式启用：

- `RING_DOCK_CONFIG`：隔离配置路径。
- `RING_DOCK_FRAME`：输出当前原始预乘 BGRA 帧（前 8 bytes 是 little-endian i32 宽、高）与布局 JSON；正式运行不要启用，避免诊断磁盘写入。
- `RING_DOCK_DROP_LOG`：明确的拖放日志文件路径；默认无日志。
- `--preview`：不嵌入桌面，临时置顶用于隔离验证；**不是正式运行参数**。

## 构建 / 回归

需要 Windows、Rust MSVC 工具链以及 Windows SDK 资源编译器。

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
.\target\release\ring-dock.exe
```

可信回归脚本（先正常退出正式实例，避免同位置两个桌面挂件干扰观察；脚本不写正式配置）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\verify.ps1 -Exe E:\tools\ring-dock\target\release\ring-dock.exe
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\verify_drag.ps1 -Exe E:\tools\ring-dock\target\release\ring-dock.exe
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\verify_desktop.ps1
python tools\orbit\verify_controlled_pixels.py
```

脚本按 PID 选择自己的窗口、使用隔离配置、结束退出测试进程。真实点击/拖放会短暂移动鼠标后恢复位置，不要在此期间操作鼠标。桌面验证用临时 DWM 观测窗，另用蓝/黄受控底色验证实际合成；截图仅用于测试，从不进入正式渲染管线。C# 辅助程序由系统 .NET Framework 编译器生成；像素检查用 Python + Pillow。

## 项目布局

- `src/render.rs`：唯一正式绘制管线、共享面板几何。
- `src/hit.rs`：与绘制共用布局的命中判定。
- `src/main.rs`：状态、交互、窗口重建与资源生命周期。
- `src/drop.rs`：OLE 文件收纳、格式/效果过滤、去重。
- `src/config.rs`：配置、参数归一化、原子保存。
- `crates/deskpin/`：桌面挂载，不掺业务渲染。
- `tools/orbit/` / `reports/orbit-glass/`：当前回归与实际画面证据。
- `tools/transparency/` / `reports/transparency/`：前一轮独立半透明验证。
- `archive/legacy-region-20261003/`：停用实现与历史文档，不参与编译，不作为当前约束。

## 验证边界

本次实际测试系统为 Windows NT 10.0.26200.0，真实宿主 `SysListView32`。当前验证不等于所有 Windows 版本、所有桌面壁纸工具或多屏 DPI 组合均已兼容。Explorer 重启与 Win+D 未在本轮主动触发；窗口自愈通过销毁本应用自身窗口验证。背景模糊、全局键盘导航和多屏布局不是本次已交付能力。

详细结果见 `reports/orbit-glass/REPORT.md`；历史“桌面不能半透明”的绝对结论已作废。

## 桌面快捷方式导入

2026-10-03 已将用户桌面、公共桌面以及桌面文件夹中的 61 个快捷方式加入正式配置，保留原有 9 个收藏；58 个程序快捷方式加入「程序」，3 个 URL 快捷方式加入「网址」。不移动/删除/改写源文件，保留原 `.lnk` / `.url` 路径（ShellExecute 保留启动参数、工作目录、协议等语义）。扫描不执行快捷方式。

复用导入工具时先正常退出正式实例，避免并发写配置。默认递归用户与公共桌面，不跟随目录重解析点。去重覆盖所有分类，不区分路径大小写；重复导入不重新写盘。无效配置直接失败，不用默认收藏覆盖。写入前生成原配置备份，使用同目录临时文件与原子替换。

```powershell
# 先预览，不修改配置
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\import_desktop.ps1 -ConfigPath E:\tools\ring-dock\target\release\config.json -ScanOnly
# 正式导入（请先退出程序）
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\import_desktop.ps1 -ConfigPath E:\tools\ring-dock\target\release\config.json
# 新功能的隔离回归
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\verify_middle_drag.ps1 -Exe E:\tools\ring-dock\target\release\ring-dock.exe
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\verify_middle_drag.ps1 -Exe E:\tools\ring-dock\target\release\ring-dock.exe -Desktop
powershell -NoProfile -ExecutionPolicy Bypass -File tools\orbit\verify_import.ps1
```

本轮证据与实际桌面图见 `reports/middle-drag/REPORT.md`。配置及 exe 改造前备份在 `target/backups/middle-drag-20261003/`，源码快照在 `target/before-middle-drag-20261003/`。回滚时不要用旧配置覆盖用户后来新增的收藏。
