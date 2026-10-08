# Ring Dock

[English](README.en.md) · 简体中文

**把常用应用、文件和文件夹放进桌面上的透明圆环。** 点击分类展开快捷面板，点一下即可打开收藏。适用于 Windows 10/11 x64，免安装，支持鼠标操作。

**轻量省内存。** 圆环和面板按实际可见范围渲染，收起时会释放面板的大型缓冲区；面板图标按需加载。在 Windows 11、2560×1392 的本机测量中，打开全部分类两轮再收起后，正式实例的私有工作集约 **12.7 MiB**。同一配置下，优化前后预览实例的私有工作集从 **26.9 MiB** 降至 **11.9 MiB**，约减少 **56%**。实际数值会随 Windows、图标和桌面环境变化；这里的私有工作集表示当前驻留在物理内存中的进程私有页，不等同于总工作集或内存保证。测量条件和数据见[内存占用说明](docs/内存占用测量.md)。

<p align="center">
  <img src="docs/images/hero-desktop.png" width="100%" alt="清新山野桌面上的 Ring Dock 圆环" />
</p>

## 下载和启动

1. 打开 **[最新版本下载页](https://github.com/stollor/ring-dock/releases/latest)**，下载 `ring-dock-v...-windows-x64.zip`。
2. 将压缩包解压到你有写入权限的文件夹，例如 `%LOCALAPPDATA%\Programs\RingDock`。
3. 双击 `ring-dock.exe`。程序会把设置保存在 exe 同目录的 `config.json`。

程序自带圆环应用图标。想放到桌面时，右键 `ring-dock.exe` 创建快捷方式；快捷方式会使用 exe 内嵌图标。

### 使用 AI 安装并整理桌面图标

复制下面的提示词，交给能操作本机 Windows、PowerShell 和桌面的 AI 助手。仓库里的安装脚本会下载并校验正式版、创建带圆环图标的桌面快捷方式、导入桌面快捷方式；AI 再按提示把桌面根目录中的普通文件和文件夹拖入分类。完整说明也可看[单独的提示词文档](docs/AI安装与图标整理提示词.md)。

````text
请在我的 Windows 电脑上安装 Ring Dock 并整理桌面图标，实际操作浏览器、PowerShell 和桌面，不要只给步骤。只使用官方仓库 https://github.com/stollor/ring-dock 。先下载并运行同一最新 Release 中的 `tools/install/install.ps1`，按脚本提示完成校验、安装和快捷方式导入；已有 `config.json` 必须保留。遇到 UAC，先说明是可选的 WinMemoryCleaner 助手并等我操作，不要替我确认。

快捷方式导入后，把桌面根目录尚未收藏的普通文件和文件夹拖入对应的展开面板；不要运行、逐个打开或移动项目，也不要扫描桌面以外的位置。根据我的桌面内容调整分类图标。最后汇报安装路径、导入数量、分类数量和配置备份位置；卸载时询问我是否保留 `config.json`。

```powershell
$repo = 'stollor/ring-dock'
$tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
$script = Join-Path $env:TEMP 'ring-dock-install.ps1'
Invoke-WebRequest "https://raw.githubusercontent.com/$repo/$tag/tools/install/install.ps1" -OutFile $script
& $script
```
````

> 首次启动时，如果检测到可选的 WinMemoryCleaner，程序会请求一次管理员权限来启动内存整理助手。拒绝授权不会阻止圆环运行，只会停用内存整理功能。

### 圆环上的信息

- **青色进度环**显示整台电脑的 CPU 使用率；**紫色进度环**显示整机物理内存使用率。
- **小圆点**是秒针标记，会随本地时间平滑移动，每分钟绕行一圈。
- **天气背景**会在启动时尝试读取 Windows 系统位置并获取当前天气；也可在设置中点击“定位并更新天气”。天气每 30 分钟自动刷新。圆心背景会按天气和昼夜显示晴天、云、雨雪与风等简单效果。
- **点击圆心**可执行一次清理：删除当前用户临时目录中超过 24 小时的文件（跳过占用文件和重解析点），并尝试让可选的 WinMemoryCleaner 整理低优先级待机内存；结果会显示在圆环上。

### 点击分类，展开对应收藏

每个扇区对应一个独立分类。点击后，收藏面板会显示该分类里的应用和快捷方式；下图以“AI 助手”分类为例。分类名称、图标和收藏内容都可以按自己的习惯调整。

<p align="center">
  <img src="docs/images/ai-assistant-panel.png" width="520" alt="Ring Dock 展开 AI 助手分类后的应用收藏面板" />
</p>

默认情况下，面板条目使用 Windows 原图标，圆环分类只显示图标、不显示文字。右键打开设置后，可以开启“登录 Windows 时自动启动 Ring Dock”，也可以修改分类图标显示方式、每个分类的图标、面板条目图标样式、时钟格式、透明度和布局。圆环图标可以自动匹配分类，也可以单独指定协作、开发、AI、娱乐、程序、文件、文件夹或网址图标。

<p align="center">
  <img src="docs/images/settings.png" width="420" alt="分类与图标设置窗口" />
</p>

## 它有什么不同

- **圆环直接贴合桌面。** 正式窗口嵌入 Windows 桌面，不会置顶盖住正在使用的应用。
- **真实逐像素透明。** 能看到圆环下方的壁纸；界面是半透明玻璃质感，不提供实时背景模糊。
- **桌面图标照常使用。** 透明区域会把点击交给下面的桌面。
- **只收藏快捷方式和路径。** 加入或移除收藏不会搬动、删除原文件。
- **设置保存在本地。** 不上传收藏列表或个人配置。

## 下载包内容

每次发布 `v` 开头的版本标签后，GitHub Actions 会在 Windows 上构建程序，并生成 Windows x64 ZIP 与 SHA-256 校验文件。下载页面只提供 Ring Dock 本身；清理助手是可选组件，未安装时圆环和快捷面板仍可使用。

## 从源码构建

开发环境需要 Windows、Rust MSVC 工具链和 Windows SDK 资源编译器：

```powershell
cargo build --release --locked
```

生成的程序位于 `target\release\ring-dock.exe`。图标资源在 `assets\ring.ico`。

## 项目说明

Ring Dock 面向 Windows 桌面，使用 Rust、Direct2D、DirectWrite 与 Windows 分层窗口。当前主要验证环境为 Windows 11；其他 Windows 版本、桌面增强软件和多屏 DPI 组合尚未逐一验证。详见[当前能力与限制](docs/当前能力与限制.md)。

本仓库尚未声明开源许可证。你可以下载和运行发布包；公开可见不等于允许复制、修改或再发布源码。

遇到问题或想提出功能建议，可以到[问题反馈页](https://github.com/stollor/ring-dock/issues)。
