# Ring Dock

**把常用应用、文件和文件夹放进桌面上的透明圆环。** 点击分类展开快捷面板，点一下即可打开收藏。适用于 Windows 10/11 x64，免安装，支持鼠标操作。

<p align="center">
  <img src="docs/images/hero-desktop.png" width="100%" alt="清新山野桌面上的 Ring Dock 圆环" />
</p>

## 下载和启动

1. 打开 **[最新版本下载页](https://github.com/stollor/ring-dock/releases/latest)**，下载 `ring-dock-v...-windows-x64.zip`。
2. 将压缩包解压到你有写入权限的文件夹，例如 `%LOCALAPPDATA%\Programs\RingDock`。
3. 双击 `ring-dock.exe`。程序会把设置保存在 exe 同目录的 `config.json`。

程序自带圆环应用图标。想放到桌面时，右键 `ring-dock.exe` 创建快捷方式；快捷方式会使用 exe 内嵌图标。也可以让 AI 按你的习惯安装并整理图标：[打开 AI 安装与图标整理提示词](docs/AI安装与图标整理提示词.md)。

> 首次启动时，如果检测到可选的 WinMemoryCleaner，程序会请求一次管理员权限来启动内存整理助手。拒绝授权不会阻止圆环运行，只会停用内存整理功能。天气查询只在用户点击设置中的定位按钮后发起。

## 怎么用

| 操作 | 效果 |
| --- | --- |
| 点击圆环上的分类 | 展开该分类的快捷面板；再点一次收起 |
| 点击收藏项 | 打开对应的应用、文件、文件夹或网址 |
| 把文件或快捷方式拖到展开的面板 | 收藏它；原文件不会移动或删除 |
| 点击“整理” | 拖动条目调整顺序，点击减号移除收藏 |
| 按住鼠标中键拖动圆环或面板 | 移动挂件；松开后记住位置 |
| 右键圆环或托盘图标 | 打开设置、重新加载配置或退出 |

右键打开设置后，可以选择分类图标的显示方式、每个分类的图标、面板条目图标样式、时钟格式、透明度和布局。圆环图标可以自动匹配分类，也可以单独指定协作、开发、AI、娱乐、程序、文件、文件夹或网址图标。

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
