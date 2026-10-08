param([string]$ReportDir='')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out=Join-Path $root 'target\desktop-import-test';New-Item -ItemType Directory -Force $out|Out-Null
$dir=Join-Path $out '快捷方式 空格';$nested=Join-Path $dir '深层 文件夹';New-Item -ItemType Directory -Force $nested|Out-Null
$targetFolder=Join-Path $out 'folder-target';New-Item -ItemType Directory -Force $targetFolder|Out-Null
$targetFile=Join-Path $out 'document.txt';Set-Content -LiteralPath $targetFile 'isolated fixture'
$shell=New-Object -ComObject WScript.Shell
try{
 foreach($v in @(@{name='程序 参数.lnk';target="$env:WINDIR\System32\notepad.exe";args='/fixture-never-launched'},@{name='文件夹.lnk';target=$targetFolder;args=''},@{name='文档.lnk';target=$targetFile;args=''})){
  $path=Join-Path $nested $v.name;$link=$shell.CreateShortcut($path);$link.TargetPath=$v.target;$link.Arguments=$v.args;$link.Save();[void][Runtime.InteropServices.Marshal]::ReleaseComObject($link)
 }
}finally{[void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell)}
$url=Join-Path $dir '站点.url';Set-Content -LiteralPath $url "[InternetShortcut]`nURL=https://example.invalid/never-opened"
Set-Content -LiteralPath (Join-Path $dir 'ignore.txt') 'not a shortcut'
$fixture=Get-Content (Join-Path $PSScriptRoot 'fixture.json') -Raw|ConvertFrom-Json
$fixture.quadrants[0].items=@(@{name='保留收藏';kind='program';target='never-launch.exe'},@{name='已存在大小写路径';kind='url';target=$url.ToUpperInvariant()})
$cfg=Join-Path $out 'config.json';$fixture|ConvertTo-Json -Depth 10|Set-Content -LiteralPath $cfg -Encoding utf8
$before=Get-Content $cfg -Raw|ConvertFrom-Json
$checks=[Collections.Generic.List[object]]::new()
function Assert($ok,$name){$checks.Add(@{name=$name;pass=[bool]$ok});if(-not $ok){throw "FAIL: $name"};Write-Host "PASS: $name"}
$scan=& (Join-Path $PSScriptRoot 'import_desktop.ps1') -ConfigPath $cfg -Roots @($dir,$nested) -ScanOnly | ConvertFrom-Json
Assert ($scan.discovered -eq 4 -and $scan.added -eq 3 -and $scan.duplicates -eq 1) '递归扫描、重叠根去重与全局大小写去重'
Assert ($scan.configBeforeSha256 -eq $scan.configAfterSha256) '扫描预览不改配置'
$result=& (Join-Path $PSScriptRoot 'import_desktop.ps1') -ConfigPath $cfg -Roots @($dir,$nested) | ConvertFrom-Json
$after=Get-Content $cfg -Raw|ConvertFrom-Json
Assert ($result.added -eq 3 -and (Test-Path -LiteralPath $result.backup)) '原子导入与原配置备份'
Assert ($after.quadrants[0].items[0].name -eq '保留收藏' -and $after.quadrants[0].items[1].name -eq '已存在大小写路径') '现有收藏顺序与条目保留'
Assert ($after.quadrants[2].items[-1].kind -eq 'folder') '文件夹快捷方式分类'
Assert ($after.quadrants[1].items[-1].kind -eq 'file') '文档快捷方式分类'
Assert ($after.quadrants[0].items[-1].target -eq (Join-Path $nested '程序 参数.lnk')) '保存原始 .lnk 路径，不丢启动参数'
$allSame=$true;foreach($s in $result.sources){if((Get-FileHash -LiteralPath $s.target).Hash -ne $s.sha256){$allSame=$false}};Assert $allSame '所有源快捷方式逐字节未改'
$again=& (Join-Path $PSScriptRoot 'import_desktop.ps1') -ConfigPath $cfg -Roots @($dir,$nested) | ConvertFrom-Json
Assert ($again.added -eq 0 -and $again.duplicates -eq 4 -and $again.configBeforeSha256 -eq $again.configAfterSha256) '重复导入幂等，不重新写盘'
$bad=Join-Path $out 'bad.json';Set-Content -LiteralPath $bad '{broken';$badHash=(Get-FileHash $bad).Hash;$rejected=$false
try{& (Join-Path $PSScriptRoot 'import_desktop.ps1') -ConfigPath $bad -Roots @($dir)|Out-Null}catch{$rejected=$true}
Assert ($rejected -and (Get-FileHash $bad).Hash -eq $badHash) '无效配置失败保护，不覆盖为默认收藏'
$report=if($ReportDir){Join-Path $ReportDir 'import-test-results.json'}else{Join-Path $out 'results.json'}
[IO.File]::WriteAllText($report,(@{date=(Get-Date).ToString('o');checks=$checks.ToArray();productionConfigModified=$false}|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
