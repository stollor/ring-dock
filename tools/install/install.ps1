[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\RingDock'),
    [switch]$ImportOnly,
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'
$repository = 'stollor/ring-dock'
$release = $null
$workDir = Join-Path $env:TEMP ('ring-dock-install-' + [guid]::NewGuid().ToString('N'))
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Ring Dock.lnk'
$configPath = Join-Path $InstallDir 'config.json'

function Import-DesktopShortcuts([string]$Tag) {
    if (-not (Test-Path -LiteralPath $configPath -PathType Leaf)) {
        Write-Output '配置文件尚未生成，已完成安装。首次启动 Ring Dock 后可再次运行本脚本的 -ImportOnly 参数来收纳桌面快捷方式。'
        return
    }

    $importer = Join-Path $workDir 'import_desktop.ps1'
    $report = Join-Path $workDir 'import-report.json'
    $uri = "https://raw.githubusercontent.com/$repository/$Tag/tools/orbit/import_desktop.ps1"
    Invoke-WebRequest -Uri $uri -OutFile $importer
    try {
        $preview = & $importer -ConfigPath $configPath -ScanOnly -ReportPath $report | ConvertFrom-Json
        Write-Output ("桌面快捷方式扫描到 {0} 项，已存在 {1} 项。" -f $preview.discovered, $preview.duplicates)
        $preview.items | Group-Object category | ForEach-Object {
            Write-Output ("  {0}: {1} 项" -f $_.Name, $_.Count)
        }
        if ($preview.discovered -gt 0) {
            $result = & $importer -ConfigPath $configPath -ReportPath $report | ConvertFrom-Json
            Write-Output ("已收藏 {0} 项；配置备份：{1}" -f $result.added, $result.backup)
        }
    } finally {
        Remove-Item -LiteralPath $report -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $importer -Force -ErrorAction SilentlyContinue
    }
}

try {
    New-Item -ItemType Directory -Force -Path $workDir | Out-Null
    if ($ImportOnly) {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/latest"
        Import-DesktopShortcuts $release.tag_name
        return
    }

    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/latest"
    $zipAsset = $release.assets | Where-Object name -Match '^ring-dock-v.+-windows-x64\.zip$' | Select-Object -First 1
    $hashAsset = $release.assets | Where-Object name -EQ 'SHA256SUMS.txt' | Select-Object -First 1
    if (-not $zipAsset -or -not $hashAsset) { throw '最新 Release 缺少 Windows x64 安装包或 SHA256SUMS.txt。' }

    $zipPath = Join-Path $workDir $zipAsset.name
    $hashPath = Join-Path $workDir 'SHA256SUMS.txt'
    Invoke-WebRequest -Uri $zipAsset.browser_download_url -OutFile $zipPath
    Invoke-WebRequest -Uri $hashAsset.browser_download_url -OutFile $hashPath
    $hashLine = Get-Content -LiteralPath $hashPath | Where-Object { $_ -match [regex]::Escape($zipAsset.name) } | Select-Object -First 1
    if ($hashLine -notmatch '^(?<hash>[A-Fa-f0-9]{64})\s+\*?(?<name>.+)$' -or $Matches.name -ne $zipAsset.name) {
        throw 'SHA256SUMS.txt 中没有安装包的有效校验记录。'
    }
    $actualHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
    if ($actualHash -ne $Matches.hash) { throw '安装包 SHA-256 校验失败，已停止安装。' }

    $stage = Join-Path $workDir 'unpacked'
    Expand-Archive -LiteralPath $zipPath -DestinationPath $stage
    $executable = Join-Path $stage 'ring-dock.exe'
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw '安装包中没有 ring-dock.exe。' }
    $targetExe = Join-Path $InstallDir 'ring-dock.exe'
    $running = Get-CimInstance Win32_Process -Filter "Name = 'ring-dock.exe'" |
        Where-Object { $_.ExecutablePath -eq $targetExe } | Select-Object -First 1
    if ($running) { throw '请先从托盘菜单退出正在运行的 Ring Dock，再重新运行安装脚本。' }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item -LiteralPath $executable -Destination $targetExe -Force
    $guide = Join-Path $stage '快速上手.txt'
    if (Test-Path -LiteralPath $guide) { Copy-Item -LiteralPath $guide -Destination (Join-Path $InstallDir '快速上手.txt') -Force }

    $shell = New-Object -ComObject WScript.Shell
    try {
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $targetExe
        $shortcut.WorkingDirectory = $InstallDir
        $shortcut.IconLocation = "$targetExe,0"
        $shortcut.Save()
    } finally {
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell)
    }

    Write-Output ("已安装 {0} 到 {1}" -f $release.tag_name, $InstallDir)
    Write-Output "已创建桌面快捷方式：$shortcutPath"
    if (-not $NoLaunch) {
        $process = Start-Process -FilePath (Join-Path $InstallDir 'ring-dock.exe') -PassThru
        $deadline = [DateTime]::UtcNow.AddSeconds(60)
        while (-not (Test-Path -LiteralPath $configPath) -and -not $process.HasExited -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 500
            $process.Refresh()
        }
        if (-not (Test-Path -LiteralPath $configPath)) {
            Write-Output '首次启动未完成配置初始化。若出现 UAC，请由用户自行处理；启动成功后运行本脚本的 -ImportOnly 参数导入桌面快捷方式。'
            return
        }
    }
    Import-DesktopShortcuts $release.tag_name
    Write-Output '桌面根目录中的普通文件和文件夹请在 Ring Dock 面板展开后用资源管理器拖入；拖放只收藏路径，不移动原件。'
} finally {
    Remove-Item -LiteralPath $workDir -Recurse -Force -ErrorAction SilentlyContinue
}
