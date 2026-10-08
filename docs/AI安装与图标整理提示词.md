# AI 安装与图标整理提示词

将以下提示词复制给能操作本机 Windows、PowerShell 和桌面的 AI 助手。它会下载安装 Ring Dock、创建使用内嵌圆环图标的桌面快捷方式，并自动收纳桌面快捷方式；桌面上的普通文件和文件夹可由 AI 通过界面拖入收藏。涉及 UAC 时，AI 必须先说明原因并等待你操作。

````text
请在我的 Windows 电脑上安装 Ring Dock，并把桌面图标收纳到圆环分类里。请实际操作浏览器、PowerShell 和 Windows 桌面，不要只给我步骤。项目官方仓库是 https://github.com/stollor/ring-dock 。

1. 从 https://github.com/stollor/ring-dock/releases/latest 下载最新的 `ring-dock-v...-windows-x64.zip` 和同一 Release 的 `SHA256SUMS.txt`。不要下载源码包或第三方 exe。用 `Get-FileHash -Algorithm SHA256` 核对 ZIP；校验不一致或没有该文件时停止。
2. 解压到 `%LOCALAPPDATA%\Programs\RingDock`。如果目录已有 `config.json`，保留它和原收藏，不要用默认配置覆盖。启动 `ring-dock.exe` 一次，并等到同目录的 `config.json` 存在。
3. 如果启动时出现 UAC，先向我说明这是为了运行可选的 WinMemoryCleaner 内存整理助手，并等待我自己决定；不要替我点击 UAC。拒绝后继续使用圆环。
4. 在桌面创建或更新 `ring-dock.exe` 的快捷方式，图标位置设为该 exe 路径、索引 0，使用程序内嵌的圆环图标。不要从网上另找图标，不要重复创建快捷方式。
5. 用下面命令下载与当前 Release 标签完全匹配的官方桌面导入脚本。把 `$installDir` 改成实际安装目录；先做只读预览，再自动导入。导入脚本递归扫描当前用户桌面和公共桌面的 `.lnk`、`.url`、`.website`、`.appref-ms` 快捷方式，按来源文件夹或目标类型分类，跳过已收藏路径；它不会执行快捷方式、移动/删除源文件或上传配置。正式导入前会在配置旁创建备份。

```powershell
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\RingDock'
$configPath = Join-Path $installDir 'config.json'
$release = Invoke-RestMethod -Uri 'https://api.github.com/repos/stollor/ring-dock/releases/latest'
$scriptPath = Join-Path $env:TEMP ("ring-dock-import-" + $release.tag_name + '.ps1')
$scriptUrl = "https://raw.githubusercontent.com/stollor/ring-dock/$($release.tag_name)/tools/orbit/import_desktop.ps1"
$reportPath = Join-Path $env:TEMP ("ring-dock-import-" + [guid]::NewGuid().ToString('N') + '.json')
Invoke-WebRequest -Uri $scriptUrl -OutFile $scriptPath

try {
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File $scriptPath -ConfigPath $configPath -ScanOnly -ReportPath $reportPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw '桌面快捷方式预览失败，停止导入。' }
    $preview = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $preview | Select-Object discovered, added, duplicates
    $preview.items | Group-Object category | Select-Object Name, Count

    if ($preview.discovered -gt 0) {
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File $scriptPath -ConfigPath $configPath -ReportPath $reportPath | Out-Null
        if ($LASTEXITCODE -ne 0) { throw '桌面快捷方式导入失败，请保留现有配置。' }
        $result = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $result | Select-Object added, duplicates, backup
    }
} finally {
    Remove-Item -LiteralPath $reportPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $scriptPath -Force -ErrorAction SilentlyContinue
}
```

6. 导入脚本只处理快捷方式。把桌面根目录中未被导入的普通文件和文件夹，通过文件资源管理器拖到圆环对应分类的展开面板里收藏；跳过回收站、“此电脑”和用于分类的文件夹本身。只添加桌面上的项目，不继续扫描其他磁盘。拖入只收藏路径，不移动或删除原件。
7. 右键圆环打开“设置”，按我的桌面内容选择分类图标和圆环显示方式。不要运行或逐个打开桌面项目。不要把完整路径、导入报告或 `config.json` 内容贴到聊天里。
8. 最后告诉我安装目录、快捷方式位置、导入数量和分类数量、配置备份位置，以及卸载方法。不要删除我的源文件；卸载时先询问我是否保留 `config.json`。
````

## 导入行为

导入器只检查当前用户桌面和公共桌面。它不会继续扫描其他磁盘。识别到的链接先按来源子目录分类：工具效率/通讯社交放入“日常协作”，开发编程/设计引擎放入“开发创作”，AI应用放入“AI助手”，游戏影音放入“娱乐影音”；其余条目按程序、文件夹、文件或网址类型匹配现有分类。桌面根目录中的普通文件和文件夹通过拖放收藏。

Release 工作流会生成 `SHA256SUMS.txt`。导入脚本从与下载包相同的 Release 标签读取，避免误用其他版本。
