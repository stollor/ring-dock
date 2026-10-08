# 当前交接：Orbit Glass（2026-10-03）

## 最高优先事实

当前正式管线是 Direct2D/DirectWrite 绘制预乘 BGRA，随后 UpdateLayeredWindow(ULW_ALPHA)。真实桌面逐像素透明已验证，不允许再凭旧探针的不可见截图推断“桌面不能透明”。旧区域裁剪、GDI 字形区域、抓壁纸伪透明均已退出编译。

- 桌面半透明基础证据：`reports/transparency/REPORT.md`。
- 当前正式版证据：`reports/orbit-glass/REPORT.md`、JSON 结果和 DWM 实际合成截图。
- 旧 HANDOFF、代码与有污染风险的脚本：`archive/legacy-region-20261003/`。只供回溯，不能照搬其约束或直接执行旧脚本。

## 关键不变量

1. DIB 为 top-down 32-bit **预乘** BGRA；RGB 不大于 alpha。整体 SourceConstantAlpha=255，不对整窗施加统一 alpha 淡化文字。
2. DirectWrite 使用灰度 AA。时钟主体不透明，玻璃背景独立 alpha；透明不等于背景模糊。
3. 不调用 SetWindowRgn / SetLayeredWindowAttributes，不抓桌面作渲染背景，不 GDI 画字。
4. 粗弧片为平头。细线可以圆头，44px 宽弧的圆头会在小角度缺口处互相叠加变成黑点。
5. 绘制与命中共用 PanelLayout：header 58、footer 32、内容裁剪、滚动上限、圆角判定一致。
6. 完全透明区域可穿透，不能靠直接 SendMessage 证明系统真实命中。现有验证区分了真实点击、命中查询、消息注入和真实 OLE 拖放。
7. 生产窗口不置顶。只在显式 --preview 下创建时设置 WS_EX_TOPMOST；临时 DWM 观测窗也可置顶，完成即销毁。
8. OLE 在 UI 线程初始化一次，初始窗口和自愈新窗口都注册 DropTarget；销毁时 RevokeDragDrop，CF_HDROP 的 STGMEDIUM 必须 ReleaseStgMedium。
9. App 唯一由 app_box 拥有；不要另构 Box::from_raw 释放同一指针。WM_DESTROY 不同步递归建窗，通过线程消息 WM_APP+42 重建。
10. 排序在抬起/捕获取消时保存，不在每次 mousemove 写盘。原子配置替换失败保留原文件并反馈；禁止测试写正式配置。

## 交互 / 限制

分类切换、关闭、整理、长按、排序、移除、滚动、自动展开拖入、去重、错误提示都已实现。收藏移除不删除原文件。只接受允许 COPY 的 CF_HDROP；拒绝仅 MOVE 和纯文本。当前不抢焦点，不宣称全局 Esc 可用；不承诺点透明外部能关面板。

动画为展开160ms淡入，没有常驻16ms重绘。诊断帧/日志由明确环境变量开启，正式版本不设这些变量。

主屏工作区与2–8分类几何有单测；多屏真实 DPI、主动重启 Explorer、Win+D 全组合尚未验证，勿冒称通过。deskpin 目前会设置祖先 WS_CLIPCHILDREN；这属于已有挂载逻辑，并非本次全新兼容性保证。

## 安全复测

先退出当前正式实例（菜单命令103），不要杀 Explorer。只运行 tools/orbit 与 tools/transparency 的当前入口。verify.ps1 的成功启动目标是隔离静默 helper；错误启动使用不存在目标，不打开用户真实程序。verify_drag 的文件也只在 target 下。实际桌面截图通过 DWM 缩略图观察真实树，蓝/黄底层独立变化验证非预先混色。

构建与复测命令见 README.md。环境硬链接增量缓存失败时 Cargo 会回退复制，这是本机文件系统警告，不是通过忽略源代码警告；Clippy 源码检查须通过。

## 回滚

本轮前完整源码/文档快照：`target/before-orbit-20261003/`。
旧运行版exe与配置：`target/backups/orbit-glass-20261003/`。
不要覆盖用户收藏。恢复程序前先正常退出当前进程，复制备份 exe 后重新启动；配置如需恢复须由用户明确选择。

## 最终清扫

已归档并移除未使用的 deskpin::repaint_desktop_area：其旧说明依赖已不存在的 blit_snapshot，还带有区域裁剪时代的桌面自擦假设。deskpin 头部改为实际挂载逻辑和验证边界，WM_DESTROY 示例改为向主消息循环投递重建，不再把 Win+D / Explorer 行为写成未经本轮验证的绝对保证。

## 2026-10-03 后续：中键位置拖动 + 桌面快捷方式导入

- `src/placement.rs` 是中键移动的纯几何。App 的 `dock_drag` 与左键排序分离；可见圆环/面板可开始移动，透明区域不开始。不改变顶层窗口工作区/桌面宿主，仍由 D2D/ULW 绘制移动后的中心。
- 只在松开中键、取消捕获/模式、退出时保存已移动位置；每个 mousemove 不写配置。左键点击/长按、滚轮在中键手势期间不触发。显式重新加载配置会丢弃旧手势、优先磁盘配置，防止覆盖外部编辑。
- `dock_position` 为兼容旧配置的可选 x/y 相对坐标；归一化后按工作区恢复并限边。面板在靠近底部时翻到上方。当前不宣称跨屏拖动。
- 用户与公共桌面递归扫描共61个源快捷方式，58个程序加入程序、3个URL加入网址。原有9个收藏与顺序保留，源文件哈希不变；正式配置目前70项（63程序/1文件/2文件夹/4网址）。这是本轮交付时的状态，用户之后调整合法。
- `tools/orbit/import_desktop.ps1` 按全局路径大小写不敏感去重，保留原shortcut路径，不执行、不搬移、不展开到所有磁盘；不跟随目录重解析点。支持 ScanOnly；原子替换前备份并检查并发配置修改。
- 新验证入口 `verify_middle_drag.ps1`（可加 -Desktop）与 `verify_import.ps1`。发布版验证、导入清单和图在 `reports/middle-drag/`。桌面验证新增 ReportDir 参数，像素验证支持报告目录参数，避免覆盖上一次的 Orbit Glass 证据。
- 中键验证的多数场景是消息注入；preview 模式包含一项带 WindowFromPoint guard 的真实系统中键拖动。不要把全部检查冒称完整鼠标 E2E，不通过点击真实用户快捷方式验证启动。

## 2026-10-03 后续：四分类按使用场景重组
- 正式配置 `target/release/config.json` 的四象限由类型划分（程序 63 / 文件 1 / 文件夹 2 / 网址 4）改为场景划分：日常协作 28 / 开发创作 25 / AI 助手 5 / 娱乐影音 12，共 70 项零丢失；条目原顺序保留，name/kind/target 逐字节未改。
- 四象限 `kind` 统一为 program（圆环图标同形，以标签与四色区分）；面板内各条目图标仍按自身 kind 绘制。另立图标需改 `src/icons.rs`，本次未动代码、无需重新构建。
- `tools/orbit/import_desktop.ps1` 改为优先按桌面来源文件夹路由（工具效率/通讯社交→日常协作，开发编程/设计引擎→开发创作，AI 应用→AI 助手，游戏影音→娱乐影音），无映射时回退到 kind 匹配再回退首象限；`verify_import.ps1` 十项全过，隔离验证了新路由。
- 回滚：`target/backups/category-20261003/config.json`；恢复后向窗口发 WM_COMMAND 101（见 `tools/orbit/control.ps1`）或右键圆环 → 重新加载配置。

## 2026-10-08 后续：分类图标与弧形文字设置
- `category_display_mode` 控制圆环分类显示为图标、弧形文字或两者；旧配置缺省为两者。
- 各 `quadrants[].category_icon` 可设为自动、协作、开发、AI、娱乐、程序、文件、文件夹或网址图标；自动模式继续依据分类名映射语义图标。
- 纯文字模式逐字沿分类弧段旋转排布，底部象限反向排字以保持正向阅读。设置窗口可编辑显示方式和每个分类图标；只在用户保存时写入配置。
