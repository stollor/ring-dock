param([string]$Exe='', [string]$ReportDir='', [switch]$Desktop)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if(-not $Exe){$Exe=Join-Path $root 'target\release\ring-dock.exe'}
$report=if($ReportDir){[IO.Path]::GetFullPath($ReportDir)}else{Join-Path $root 'reports\middle-drag'};New-Item -ItemType Directory -Force $report|Out-Null
. (Join-Path $PSScriptRoot 'control.ps1')
[OrbitControl]::SetThreadDpiAwarenessContext([IntPtr](-4))|Out-Null
$mode=if($Desktop){'desktop'}else{'preview'}
$out=Join-Path $root ('target\middle-drag-test-'+$mode);New-Item -ItemType Directory -Force $out|Out-Null
$cfgPath=Join-Path $out 'config.json';Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'fixture.json') -Destination $cfgPath
$cfg=Get-Content $cfgPath -Raw|ConvertFrom-Json
$cfg.quadrants[0].items=@(1..40|ForEach-Object{@{name="隔离收藏 $_";kind='program';target="Z:\__never_launch_middle_$_"}})
$cfg|ConvertTo-Json -Depth 10|Set-Content $cfgPath -Encoding utf8
$checks=[Collections.Generic.List[object]]::new();$p=$null
function Assert($ok,$name){$checks.Add(@{name=$name;pass=[bool]$ok});if(-not $ok){throw "FAIL: $name"};Write-Host "PASS: $name"}
function Meta {Get-Content (Join-Path $out 'frame.bgra.json') -Raw|ConvertFrom-Json}
function WaitFrame {Start-Sleep -Milliseconds 230}
function Msg($msg,$x,$y,$button=0){[OrbitControl]::Message($script:h,$msg,$button,[OrbitControl]::Pack([int]$x,[int]$y))}
function CenterIs($center,$name){$m=Meta;Assert ([Math]::Abs($m.center[0]-$center[0]) -lt 1.1 -and [Math]::Abs($m.center[1]-$center[1]) -lt 1.1) $name}
function ItemsHash { $v=Get-Content $cfgPath -Raw|ConvertFrom-Json;( @($v.quadrants | ForEach-Object { [ordered]@{label=$_.label;kind=$_.kind;items=@($_.items | ForEach-Object { [ordered]@{name=$_.name;kind=$_.kind;target=$_.target} })} }) | ConvertTo-Json -Depth 10 -Compress) }
$oldCfg=$env:RING_DOCK_CONFIG;$oldFrame=$env:RING_DOCK_FRAME;$oldLog=$env:RING_DOCK_DROP_LOG
function StartTarget {
 $env:RING_DOCK_CONFIG=$cfgPath;$env:RING_DOCK_FRAME=Join-Path $out 'frame.bgra';$env:RING_DOCK_DROP_LOG=$null
 try{
  $splat=@{FilePath=$Exe;PassThru=$true;WindowStyle='Hidden';RedirectStandardError=(Join-Path $out 'errors.txt')};if(-not $Desktop){$splat.ArgumentList='--preview'}
  $script:p=Start-Process @splat
 }finally{$env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame;$env:RING_DOCK_DROP_LOG=$oldLog}
 for($i=0;$i -lt 40;$i++){Start-Sleep -Milliseconds 100;$script:h=[OrbitControl]::Find($p.Id);if($script:h -ne [IntPtr]::Zero){break}}
 Assert ($script:h -ne [IntPtr]::Zero) '找到隔离配置、PID定位的目标';WaitFrame
}
function QuitTarget {if($p -and -not $p.HasExited){[OrbitControl]::Message($script:h,0x111,103,0);Assert ($p.WaitForExit(4000) -and $p.ExitCode -eq 0) '正常退出码0'}}
try{
 StartTarget
 $m=Meta;$start=@($m.center[0],$m.center[1]);$before=(Get-FileHash $cfgPath).Hash;$itemsBefore=ItemsHash
 Msg 0x207 4 4 16;Msg 0x200 200 200 16;Msg 0x208 200 200;WaitFrame;CenterIs $start '透明区域消息不启动移动'
 Msg 0x207 $start[0] $start[1] 16;WaitFrame;CenterIs $start '中键按下不跳位'
 Msg 0x200 ($start[0]+120) ($start[1]+40) 16;WaitFrame;$moved=@(($start[0]+120),($start[1]+40));CenterIs $moved '中键按住移动整个圆环'
 Assert ((Get-FileHash $cfgPath).Hash -eq $before) '移动期间不写配置'
 Msg 0x201 $moved[0] $moved[1] 1;Start-Sleep -Milliseconds 620;Msg 0x202 $moved[0] $moved[1];WaitFrame
 Assert ([OrbitControl]::Title($script:h) -eq 'ring-dock') '中键期间左键/长按不误触'
 Msg 0x208 $moved[0] $moved[1];WaitFrame
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;$m=Meta
 Assert ([Math]::Abs($saved.dock_position.x*$m.width-$m.center[0]) -lt 0.1 -and [Math]::Abs($saved.dock_position.y*$m.height-$m.center[1]) -lt 0.1) '松开中键保存相对位置'
 Assert ((ItemsHash) -eq $itemsBefore) '移动不改分类、收藏或顺序'
 $hash=(Get-FileHash $cfgPath).Hash;Msg 0x207 $moved[0] $moved[1] 16;Msg 0x208 $moved[0] $moved[1];WaitFrame
 Assert ((Get-FileHash $cfgPath).Hash -eq $hash) '原地中键单击不写配置、不切分类'
 QuitTarget;StartTarget;CenterIs $moved '重启恢复圆环位置'
 $m=Meta;$rx=$m.center[0]+$m.radius/[Math]::Sqrt(2);$ry=$m.center[1]-$m.radius/[Math]::Sqrt(2)
 [OrbitControl]::Click($script:h,[int]$rx,[int]$ry);WaitFrame;$m=Meta;$l=$m.panel
 Assert ($null -ne $l) '移动后分类命中仍正确'
 $px=$l.x+24;$py=$l.y+24;$oldPanel=@($l.x,$l.y);$oldCenter=@($m.center[0],$m.center[1])
 Msg 0x207 $px $py 16;Msg 0x200 ($px+60) ($py-30) 16;WaitFrame
 CenterIs @(($oldCenter[0]+60),($oldCenter[1]-30)) '面板空白处中键也能移动'
 $m=Meta;Assert ([Math]::Abs($m.panel.x-$oldPanel[0]-60) -lt 1.1 -and [Math]::Abs($m.panel.y-$oldPanel[1]+30) -lt 1.1) '展开面板跟随移动'
 [OrbitControl]::Message($script:h,0x20A,(-120 -shl 16),0);WaitFrame;Assert ((Meta).scroll -eq 0) '移动期间不误滚动'
 Msg 0x208 ($px+60) ($py-30);[OrbitControl]::Message($script:h,0x20A,(-120 -shl 16),0);WaitFrame;Assert ((Meta).scroll -gt 0) '松开后滚轮仍可滚动收藏'
 $m=Meta;Msg 0x207 $m.center[0] $m.center[1] 16;Msg 0x200 ($m.width-1) ($m.height-1) 16;WaitFrame
 $edge=Meta;Assert ($edge.center[0] -le $edge.width-$edge.outer-12 -and $edge.center[1] -le $edge.height-$edge.outer-12) '右下边界限制，不把圆环拖出工作区'
 Assert ($edge.panel.y+$edge.panel.h -lt $edge.center[1]-$edge.outer -and $edge.panel.x+$edge.panel.w -le $edge.width-12) '底边面板翻到上方且仍在工作区'
 Msg 0x208 ($m.width-1) ($m.height-1);WaitFrame
 $m=Meta;Msg 0x207 $m.center[0] $m.center[1] 16;Msg 0x200 0 0 16;WaitFrame
 $m=Meta;Assert ($m.center[0] -ge $m.outer+12 -and $m.center[1] -ge $m.outer+12) '左上/负坐标边界限制'
 [OrbitControl]::Message($script:h,0x1F,0,0);WaitFrame;$cancelled=@((Meta).center[0],(Meta).center[1]);Msg 0x200 600 600 0;WaitFrame;CenterIs $cancelled '取消捕获后不继续跟随鼠标'
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($null -ne $saved.dock_position) '取消手势仍持久化已移动位置'
 # Restore a safe central position for the real OS input guard.
 $cfg=Get-Content $cfgPath -Raw|ConvertFrom-Json;$cfg.dock_position.x=.5;$cfg.dock_position.y=.4;$cfg|ConvertTo-Json -Depth 10|Set-Content $cfgPath -Encoding utf8
 [OrbitControl]::Message($script:h,0x111,101,0);WaitFrame;$m=Meta
 if(-not $Desktop){
  $realStart=@($m.center[0],$m.center[1]);[OrbitControl]::RealMiddleDrag($script:h,[int]$m.center[0],[int]$m.center[1],90,45);WaitFrame
  CenterIs @(($realStart[0]+90),($realStart[1]+45)) '真实系统中键拖动（窗口命中guard）'
 }
 if($Desktop){
  $m=Meta;Msg 0x207 $m.center[0] $m.center[1] 16;Msg 0x200 ($m.center[0]+80) ($m.center[1]+30) 16;Msg 0x208 ($m.center[0]+80) ($m.center[1]+30);WaitFrame
 }
 $m=Meta;[OrbitControl]::Click($script:h,[int]($m.center[0]+$m.radius/[Math]::Sqrt(2)),[int]($m.center[1]-$m.radius/[Math]::Sqrt(2)));WaitFrame
 if($Desktop){
  $m=Meta;$observer=Join-Path $root 'target\orbit-test\desktop-observer.exe';if(Test-Path $observer){
   $width=[int]($m.panel.w+40);$x=[int]($m.center[0]-$width/2);$y=[int]($m.center[1]-195);$height=[int]($m.panel.y+$m.panel.h+20-$y);$prefix=Join-Path $report 'middle-drag-expanded'
   $obs=Start-Process $observer -ArgumentList @($script:h.ToInt64(),$x,$y,$width,$height,('"'+$prefix+'"')) -PassThru -WindowStyle Hidden
   Assert ($obs.WaitForExit(5000) -and $obs.ExitCode -eq 0) 'DWM真实桌面合成截图'
  }
 }
 Assert ([string]::IsNullOrWhiteSpace((Get-Content (Join-Path $out 'errors.txt') -Raw))) '中键/重启/边界渲染无错误'
 QuitTarget
}finally{
 $env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame;$env:RING_DOCK_DROP_LOG=$oldLog
 if($p -and -not $p.HasExited){$hh=[OrbitControl]::Find($p.Id);if($hh -ne [IntPtr]::Zero){[OrbitControl]::Message($hh,0x111,103,0)};if(-not $p.WaitForExit(3000)){Stop-Process -Id $p.Id}}
 [IO.File]::WriteAllText((Join-Path $report ('middle-drag-'+$mode+'-results.json')),(@{date=(Get-Date).ToString('o');checks=$checks.ToArray();realMiddleInput=(-not [bool]$Desktop);desktopEmbedded=[bool]$Desktop;productionConfigModified=$false}|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
}
