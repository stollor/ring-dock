# AI 安装与图标整理提示词

将以下提示词复制给能操作本机 Windows、PowerShell 和桌面的 AI 助手。仓库内的安装脚本负责下载、校验、解压、创建桌面快捷方式和导入桌面快捷方式；AI 只需处理桌面上的普通文件和文件夹。涉及 UAC 时，AI 必须先说明原因并等待你操作。

````text
请在我的 Windows 电脑上安装 Ring Dock 并整理桌面图标，实际操作浏览器、PowerShell 和桌面，不要只给步骤。只使用官方仓库 https://github.com/stollor/ring-dock 。运行仓库 `tools/install/install.ps1` 完成正式版下载、SHA-256 校验、安装、桌面快捷方式创建和桌面快捷方式导入。已有 `config.json` 必须保留。遇到 UAC，先说明是可选的 WinMemoryCleaner 助手并等我操作，不要替我确认。

快捷方式导入后，把桌面根目录尚未收藏的普通文件和文件夹拖入对应的展开面板；不要运行、逐个打开或移动项目，也不要扫描桌面以外的位置。根据我的桌面内容调整分类图标。最后汇报安装路径、导入数量、分类数量和配置备份位置；卸载时询问我是否保留 `config.json`。

```powershell
$repo = 'stollor/ring-dock'
$tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
$script = Join-Path $env:TEMP 'ring-dock-install.ps1'
Invoke-WebRequest "https://raw.githubusercontent.com/$repo/$tag/tools/install/install.ps1" -OutFile $script
& $script
```
````

## 导入行为

导入器只检查当前用户桌面和公共桌面。它不会继续扫描其他磁盘。识别到的链接先按来源子目录分类：工具效率/通讯社交放入“日常协作”，开发编程/设计引擎放入“开发创作”，AI应用放入“AI助手”，游戏影音放入“娱乐影音”；其余条目按程序、文件夹、文件或网址类型匹配现有分类。桌面根目录中的普通文件和文件夹通过拖放收藏。

Release 工作流会生成 `SHA256SUMS.txt`。导入脚本从与下载包相同的 Release 标签读取，避免误用其他版本。
安装流程实现在 [`tools/install/install.ps1`](../tools/install/install.ps1)，提示词不再内嵌完整下载和导入命令。
