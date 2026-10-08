param([string]$Exe='')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if(-not $Exe){$Exe=Join-Path $root 'target\debug\ring-dock.exe'}
. (Join-Path $PSScriptRoot 'control.ps1')
[OrbitControl]::SetThreadDpiAwarenessContext([IntPtr](-4))|Out-Null
$out=Join-Path $root 'target\orbit-drag-test';New-Item -ItemType Directory -Force $out|Out-Null
$base=Join-Path $root 'target\release\config.json';if(-not (Test-Path $base)){$base=Join-Path $PSScriptRoot 'fixture.json'}
$cfg=Get-Content $base -Raw|ConvertFrom-Json
$cfg.quadrants[1].items=@();$cfgPath=Join-Path $out 'config.json';$cfg|ConvertTo-Json -Depth 10|Set-Content $cfgPath -Encoding utf8
Set-Content (Join-Path $out 'drop-one.txt') 'isolated drag test';Set-Content (Join-Path $out 'drop-two.txt') 'recreated target drag test'
$checks=[Collections.Generic.List[object]]::new()
function Assert($ok,$name){$checks.Add(@{name=$name;pass=[bool]$ok});if(-not $ok){throw "FAIL: $name"};Write-Host "PASS: $name"}
function Meta {Get-Content (Join-Path $out 'frame.bgra.json') -Raw|ConvertFrom-Json}
function Drag($file,$hover,$effects=1,$text=$false){
 $m=Meta;$x=[int]($m.center[0]-164+60);$y=[int]($m.center[1]+$m.outer+20+82)
 if($hover){$hx=[int]($m.center[0]+$m.radius/[Math]::Sqrt(2));$hy=[int]($m.center[1]+$m.radius/[Math]::Sqrt(2))}else{$hx=$x;$hy=$y}
 $dest=[OrbitControl+POINT]::new($x,$y);$start=[OrbitControl+POINT]::new($hx,$hy);[OrbitControl]::ClientToScreen($script:h,[ref]$dest)|Out-Null;[OrbitControl]::ClientToScreen($script:h,[ref]$start)|Out-Null
 $log=& (Join-Path $PSScriptRoot 'drag_source.ps1') -Path $file -X $dest.x -Y $dest.y -HoverX $start.x -HoverY $start.y -Effects $effects -TextData:$text
 $log|Add-Content (Join-Path $out 'ole.log');Write-Host $log;$expected=if($effects -eq 1 -and -not $text){1}else{0};Assert ($log -match ('DoDragDrop effect='+$expected)) "真实 OLE DoDragDrop effect=$expected"
 Start-Sleep -Milliseconds 300
}
$oldCfg=$env:RING_DOCK_CONFIG;$oldFrame=$env:RING_DOCK_FRAME;$oldLog=$env:RING_DOCK_DROP_LOG
try{
 $env:RING_DOCK_CONFIG=$cfgPath;$env:RING_DOCK_FRAME=Join-Path $out 'frame.bgra';$env:RING_DOCK_DROP_LOG=Join-Path $out 'drop.log'
 $p=Start-Process $Exe -ArgumentList '--preview' -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $out 'errors.txt')
 $env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame;$env:RING_DOCK_DROP_LOG=$oldLog
 for($i=0;$i -lt 30;$i++){Start-Sleep -Milliseconds 100;$script:h=[OrbitControl]::Find($p.Id);if($script:h -ne [IntPtr]::Zero){break}}
 Assert ($script:h -ne [IntPtr]::Zero) '隔离测试窗口';[OrbitControl]::ShowWindow($script:h,4)|Out-Null;Start-Sleep -Milliseconds 300
 Drag (Join-Path $out 'drop-one.txt') $true
 Assert ([OrbitControl]::Title($script:h) -eq 'ring-dock#expanded=1') '拖经文件分类悬停自动展开'
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($saved.quadrants[1].items.Count -eq 1) '拖入面板并持久化'
 Drag (Join-Path $out 'drop-one.txt') $false
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($saved.quadrants[1].items.Count -eq 1) '重复拖入不重复添加'
 $old=$script:h;[OrbitControl]::Message($script:h,0x10,0,0);Start-Sleep -Milliseconds 400;$script:h=[OrbitControl]::Find($p.Id);Assert ($script:h -ne [IntPtr]::Zero -and $script:h -ne $old) '新窗口创建';[OrbitControl]::ShowWindow($script:h,4)|Out-Null
 Drag (Join-Path $out 'drop-two.txt') $false
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($saved.quadrants[1].items.Count -eq 2) '重建窗口重新注册拖放成功'
 Drag (Join-Path $out 'drop-two.txt') $false 2
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($saved.quadrants[1].items.Count -eq 2) '拒绝仅允许 MOVE 的源，不移动原文件'
 Drag (Join-Path $out 'drop-two.txt') $false 1 $true
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($saved.quadrants[1].items.Count -eq 2) '拒绝非 CF_HDROP 文本源'
 Assert ([string]::IsNullOrWhiteSpace((Get-Content (Join-Path $out 'errors.txt') -Raw))) '无 OLE 注册/渲染错误'
}finally{
 $env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame;$env:RING_DOCK_DROP_LOG=$oldLog
 if($p -and -not $p.HasExited){$h=[OrbitControl]::Find($p.Id);if($h -ne [IntPtr]::Zero){[OrbitControl]::Message($h,0x111,103,0)};if(-not $p.WaitForExit(3000)){Stop-Process -Id $p.Id}}
 @{date='2026-10-03';checks=$checks.ToArray();productionConfigModified=$false;realOLE=$true}|ConvertTo-Json -Depth 6|Set-Content (Join-Path $out 'drag-results.json') -Encoding utf8
}
