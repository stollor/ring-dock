param([string]$Exe='', [string]$ReportDir='')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if(-not $Exe){$Exe=Join-Path $root 'target\release\ring-dock.exe'}
. (Join-Path $PSScriptRoot 'control.ps1')
[OrbitControl]::SetThreadDpiAwarenessContext([IntPtr](-4))|Out-Null
$out=Join-Path $root 'target\orbit-desktop-test';New-Item -ItemType Directory -Force $out|Out-Null
$report=if($ReportDir){[IO.Path]::GetFullPath($ReportDir)}else{Join-Path $root 'reports\orbit-glass'};New-Item -ItemType Directory -Force $report|Out-Null
$observerDir=Join-Path $root 'target\orbit-test';New-Item -ItemType Directory -Force $observerDir|Out-Null
& (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe') /nologo /target:exe /platform:x64 /r:System.Windows.Forms.dll /r:System.Drawing.dll ("/win32manifest:"+(Join-Path $root 'assets\app.manifest')) ("/out:"+(Join-Path $observerDir 'desktop-observer.exe')) (Join-Path $PSScriptRoot 'desktop_observer.cs')
if($LASTEXITCODE -ne 0){throw 'Observer compilation failed'}
$cfgPath=Join-Path $out 'isolated-config.json';$base=Join-Path $root 'target\release\config.json';if(-not (Test-Path $base)){$base=Join-Path $PSScriptRoot 'fixture.json'};Copy-Item $base $cfgPath
$checks=[Collections.Generic.List[object]]::new()
function Assert($ok,$name){$checks.Add(@{name=$name;pass=[bool]$ok});if(-not $ok){throw "FAIL: $name"};Write-Host "PASS: $name"}
function Meta {Get-Content (Join-Path $out 'frame.bgra.json') -Raw|ConvertFrom-Json}
function Capture($name,$controlled=$false){
 Start-Sleep -Milliseconds 300;$m=Meta;$panel=$m.panel
 $width=if($panel){[int][Math]::Max(400,$panel.w+40)}else{400};$height=if($panel){[int]($panel.y+$panel.h+20-($m.center[1]-195))}else{390}
 $x=[int]($m.center[0]-$width/2);$y=[int]($m.center[1]-195)
 $prefix=Join-Path $report $name
 $args=@($script:h.ToInt64(),$x,$y,$width,$height,('"'+$prefix+'"'));if($controlled){$args+='--controlled';$args+=('"'+(Join-Path $out 'frame.bgra')+'"')}
 $before=(Get-FileHash (Join-Path $out 'frame.bgra')).Hash
 $p2=Start-Process (Join-Path $root 'target\orbit-test\desktop-observer.exe') -ArgumentList $args -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $out 'observer-errors.txt')
 if(-not $p2.WaitForExit(5000)){Stop-Process -Id $p2.Id;throw 'Observer timed out'}
 Assert ($p2.ExitCode -eq 0) "DWM 实际桌面截图 $name"
 if($controlled){Assert ((Get-Content ($prefix+'-source-stability.json') -Raw|ConvertFrom-Json).foregroundNotRedrawnBetweenBlueAndYellow) '底层蓝黄改变期间前景未重绘（帧文件时间戳）'}
 Copy-Item (Join-Path $out 'frame.bgra') (Join-Path $out "$name.bgra")
 @{x=$x;y=$y;width=$width;height=$height}|ConvertTo-Json|Set-Content (Join-Path $report "$name-roi.json") -Encoding utf8
}
$oldCfg=$env:RING_DOCK_CONFIG;$oldFrame=$env:RING_DOCK_FRAME
try{
 $env:RING_DOCK_CONFIG=$cfgPath;$env:RING_DOCK_FRAME=Join-Path $out 'frame.bgra'
 $p=Start-Process $Exe -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $out 'errors.txt')
 $env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame
 for($i=0;$i -lt 30;$i++){Start-Sleep -Milliseconds 100;$script:h=[OrbitControl]::Find($p.Id);if($script:h -ne [IntPtr]::Zero){break}}
 Assert ($script:h -ne [IntPtr]::Zero) '正式 release 进程窗口'
 [OrbitControl]::ShowWindow($script:h,4)|Out-Null;Start-Sleep -Milliseconds 400
 $parent=[OrbitControl]::GetParent($script:h);Assert ($parent -ne [IntPtr]::Zero) "已嵌入真实桌面宿主 $([OrbitControl]::Class($parent))"
 Assert (([OrbitControl]::GetWindowLongW($script:h,-20) -band 8) -eq 0) '正式桌面模式非 TOPMOST'
 Capture 'desktop-closed';Capture 'desktop-alpha' $true
 $m=Meta;[OrbitControl]::Click($script:h,[int]($m.center[0]+$m.radius/[Math]::Sqrt(2)),[int]($m.center[1]-$m.radius/[Math]::Sqrt(2)))
 Assert ([OrbitControl]::Title($script:h) -eq 'ring-dock#expanded=0') '桌面嵌入分类展开';Capture 'desktop-expanded'
 $l=(Meta).panel;[OrbitControl]::Click($script:h,[int]($l.x+$l.w-68),[int]($l.y+27));Capture 'desktop-editing'
 Assert ([string]::IsNullOrWhiteSpace((Get-Content (Join-Path $out 'errors.txt') -Raw))) '正式桌面渲染无错误'
}finally{
 $env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame
 if($p -and -not $p.HasExited){$h=[OrbitControl]::Find($p.Id);if($h -ne [IntPtr]::Zero){[OrbitControl]::Message($h,0x111,103,0)};if(-not $p.WaitForExit(3000)){Stop-Process -Id $p.Id}}
 @{date='2026-10-03';checks=$checks.ToArray();productionConfigModified=$false;realDesktop=$true}|ConvertTo-Json -Depth 6|Set-Content (Join-Path $report 'desktop-results.json') -Encoding utf8
}
