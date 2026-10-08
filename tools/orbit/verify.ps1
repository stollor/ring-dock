param([string]$Exe='',[switch]$SkipRealClick)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if(-not $Exe){$Exe=Join-Path $root 'target\debug\ring-dock.exe'}
. (Join-Path $PSScriptRoot 'control.ps1')
[OrbitControl]::SetThreadDpiAwarenessContext([IntPtr](-4))|Out-Null
$out=Join-Path $root 'target\orbit-test';New-Item -ItemType Directory -Force $out|Out-Null
$marker=Join-Path $out 'launch-marker.txt';if(Test-Path $marker){Remove-Item -LiteralPath $marker}
$helper=Join-Path $out 'launch-helper.exe';$helperSource=Join-Path $out 'launch-helper.cs'
('using System;using System.IO;public class Entry { [STAThread]public static void Main(){File.WriteAllText(@"'+$marker+'","launched");}}')|Set-Content $helperSource -Encoding utf8
& (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe') /nologo /target:winexe /platform:x64 "/out:$helper" $helperSource
if($LASTEXITCODE -ne 0){throw 'Launch helper compile failed'}
$base=Join-Path $root 'target\release\config.json';if(-not (Test-Path $base)){$base=Join-Path $PSScriptRoot 'fixture.json'}
$cfg=Get-Content $base -Raw|ConvertFrom-Json
$cfg.max_columns=4
$cfg.quadrants[0].items=@(1..35|ForEach-Object{@{name="收藏 $_";kind='program';target="Z:\__orbit_missing_$_"}})
$cfg.quadrants[1].items=@();$cfgPath=Join-Path $out 'isolated-config.json';$cfg|ConvertTo-Json -Depth 10|Set-Content $cfgPath -Encoding utf8
$checks=[Collections.Generic.List[object]]::new()
function Assert($ok,$name){$checks.Add(@{name=$name;pass=[bool]$ok});if(-not $ok){throw "FAIL: $name"};Write-Host "PASS: $name"}
function Meta { Get-Content (Join-Path $out 'frame.bgra.json') -Raw|ConvertFrom-Json }
function WaitFrame {Start-Sleep -Milliseconds 240}
function State($suffix){$actual=[OrbitControl]::Title($script:h);Assert ($actual -eq "ring-dock$suffix") "状态 $suffix (actual=$actual, handle=$script:h)"}
function Snapshot($name){Copy-Item (Join-Path $out 'frame.bgra') (Join-Path $out "$name.bgra");Copy-Item (Join-Path $out 'frame.bgra.json') (Join-Path $out "$name.bgra.json")}
function RingClick($q){$m=Meta;$a=(-45+90*$q)*[Math]::PI/180;$x=[int]($m.center[0]+$m.radius*[Math]::Cos($a));$y=[int]($m.center[1]+$m.radius*[Math]::Sin($a));[OrbitControl]::Click($script:h,$x,$y);WaitFrame}
$oldCfg=$env:RING_DOCK_CONFIG;$oldFrame=$env:RING_DOCK_FRAME
try{
 $env:RING_DOCK_CONFIG=$cfgPath;$env:RING_DOCK_FRAME=Join-Path $out 'frame.bgra'
 $p=Start-Process -FilePath $Exe -ArgumentList '--preview' -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $out 'errors.txt')
 $env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame
 for($i=0;$i -lt 30;$i++){Start-Sleep -Milliseconds 100;$script:h=[OrbitControl]::Find($p.Id);if($script:h -ne [IntPtr]::Zero){break}}
 Assert ($script:h -ne [IntPtr]::Zero) '找到本次进程窗口';WaitFrame
 Assert (([OrbitControl]::GetWindowLongW($script:h,-20) -band 0x80000) -ne 0) '正式窗口样式 WS_EX_LAYERED'
 State '';Snapshot 'closed'
 if(-not $SkipRealClick){
  [OrbitControl]::ShowWindow($script:h,4)|Out-Null
  [OrbitControl]::SetWindowPos($script:h,[IntPtr](-1),0,0,0,0,0x13)|Out-Null
  Start-Sleep -Milliseconds 200
  $m=Meta;$x=[int]($m.center[0]+$m.radius/[Math]::Sqrt(2));$y=[int]($m.center[1]-$m.radius/[Math]::Sqrt(2))
  $pt=[OrbitControl+POINT]::new($x,$y);[OrbitControl]::ClientToScreen($script:h,[ref]$pt)|Out-Null
  $wr=[OrbitControl+RECT]::new();[OrbitControl]::GetWindowRect($script:h,[ref]$wr)|Out-Null;Write-Host "DEBUG visible=$([OrbitControl]::IsWindowVisible($script:h)) rect=$($wr.l),$($wr.t),$($wr.r),$($wr.b) observedClass=$([OrbitControl]::Class([OrbitControl]::WindowFromPoint($pt))) ex=$([OrbitControl]::GetWindowLongW($script:h,-20))"
  Assert ([OrbitControl]::WindowFromPoint($pt) -eq $script:h) "实际窗口半透明环带可命中 (observed=$([OrbitControl]::WindowFromPoint($pt)), target=$script:h, xy=$($pt.x),$($pt.y))"
  [OrbitControl]::RealClick($script:h,$x,$y);WaitFrame;State '#expanded=0'
  [OrbitControl]::RealClick($script:h,$x,$y);WaitFrame;State ''
  $pt=[OrbitControl+POINT]::new([int]$m.center[0],[int]($m.center[1]-$m.outer-20));[OrbitControl]::ClientToScreen($script:h,[ref]$pt)|Out-Null
  Assert ([OrbitControl]::WindowFromPoint($pt) -ne $script:h) '外部透明像素穿透（仅查询，不点击下层）'
 }
 RingClick 0;State '#expanded=0';Snapshot 'expanded'
 RingClick 1;State '#expanded=1';Snapshot 'empty'
 $m=Meta;$l=$m.panel;[OrbitControl]::Click($script:h,[int]($l.x+24),[int]($l.y+23));State '#expanded=1'
 RingClick 0;State '#expanded=0'
 $m=Meta;$l=$m.panel;[OrbitControl]::Click($script:h,[int]($l.x+$l.w-68),[int]($l.y+27));WaitFrame;State '#expanded=0#edit';Snapshot 'editing'
 $m=Meta;$one=$m.panel.cells[0];$two=$m.panel.cells[1]
 [OrbitControl]::Message($script:h,0x201,1,[OrbitControl]::Pack([int]($one[0]+20),[int]($one[1]+20)))
 [OrbitControl]::Message($script:h,0x200,1,[OrbitControl]::Pack([int]($two[0]+20),[int]($two[1]+20)))
 $deadline=(Get-Date).AddSeconds(3)
 do { if((Meta).panel.itemNames[1] -eq '收藏 1'){break};Start-Sleep -Milliseconds 20 } while((Get-Date) -lt $deadline)
 Assert ((Meta).panel.itemNames[1] -eq '收藏 1') '排序绘制状态已更新'
 [OrbitControl]::Message($script:h,0x202,0,[OrbitControl]::Pack([int]($two[0]+20),[int]($two[1]+20)))
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($saved.quadrants[0].items[1].name -eq '收藏 1') '拖动排序持久化'
 $m=Meta;$one=$m.panel.cells[0];[OrbitControl]::Click($script:h,[int]($one[0]+$one[2]-6),[int]($one[1]+7));WaitFrame
 $saved=Get-Content $cfgPath -Raw|ConvertFrom-Json;Assert ($saved.quadrants[0].items.Count -eq 34) '删除角标持久化'
 $l=(Meta).panel;[OrbitControl]::Click($script:h,[int]($l.x+$l.w-68),[int]($l.y+27));WaitFrame;State '#expanded=0'
 [OrbitControl]::Message($script:h,0x20A,(-120 -shl 16),0);WaitFrame;Assert ((Meta).scroll -gt 0) '面板内部滚动'
 RingClick 0;RingClick 0
 $one=(Meta).panel.cells[0];[OrbitControl]::Message($script:h,0x201,1,[OrbitControl]::Pack([int]($one[0]+20),[int]($one[1]+20)));Start-Sleep -Milliseconds 620
 [OrbitControl]::Message($script:h,0x202,0,[OrbitControl]::Pack([int]($one[0]+20),[int]($one[1]+20)));WaitFrame;State '#expanded=0#edit'
 $l=(Meta).panel;[OrbitControl]::Click($script:h,[int]($l.x+$l.w-25),[int]($l.y+27));WaitFrame;State ''
 RingClick 0;$one=(Meta).panel.cells[0];[OrbitControl]::Click($script:h,[int]($one[0]+20),[int]($one[1]+20));WaitFrame;State '#expanded=0';Snapshot 'launch-error'
 Assert ([string]::IsNullOrWhiteSpace((Get-Content (Join-Path $out 'errors.txt') -Raw))) '无渲染/拖放注册错误'
 # Launch a silent disposable helper, never a real user app.
 $modified=Get-Content $cfgPath -Raw|ConvertFrom-Json
 $modified.quadrants[2].items=@(@{name='隔离启动测试';kind='program';target=$helper})
 $modified|ConvertTo-Json -Depth 10|Set-Content $cfgPath -Encoding utf8
 [OrbitControl]::Message($script:h,0x111,101,0);WaitFrame;RingClick 2
 $one=(Meta).panel.cells[0];[OrbitControl]::Click($script:h,[int]($one[0]+20),[int]($one[1]+20));WaitFrame
 State '';Assert (Test-Path $marker) '成功启动目标并自动收起（静默隔离 helper）'
 # Compatibility preference: click a different sector closes instead of switching.
 $modified.switch_panel_on_click=$false;$modified.auto_collapse_after_open=$false
 $modified|ConvertTo-Json -Depth 10|Set-Content $cfgPath -Encoding utf8
 [OrbitControl]::Message($script:h,0x111,101,0);WaitFrame;RingClick 0;RingClick 1;State ''
 RingClick 2;$one=(Meta).panel.cells[0];[OrbitControl]::Click($script:h,[int]($one[0]+20),[int]($one[1]+20));WaitFrame;State '#expanded=2'
 # Destroy only our app window: exercises the same recreate path without restarting Explorer.
 $old=$script:h;[OrbitControl]::Message($script:h,0x10,0,0);WaitFrame;$script:h=[OrbitControl]::Find($p.Id)
 Assert ($script:h -ne [IntPtr]::Zero -and $script:h -ne $old) '窗口销毁后自愈（未重启 Explorer）'
 [OrbitControl]::Message($script:h,0x111,103,0);Assert ($p.WaitForExit(3000)) '正常退出且无双重释放'
 Assert ($p.ExitCode -eq 0) '退出码 0'
}finally{
 $env:RING_DOCK_CONFIG=$oldCfg;$env:RING_DOCK_FRAME=$oldFrame
 if($p -and -not $p.HasExited){$hh=[OrbitControl]::Find($p.Id);if($hh -ne [IntPtr]::Zero){[OrbitControl]::Message($hh,0x111,103,0)};if(-not $p.WaitForExit(3000)){Stop-Process -Id $p.Id}}
 @{date='2026-10-03';checks=$checks.ToArray();realClicksTested=(-not $SkipRealClick);productionConfigModified=$false}|ConvertTo-Json -Depth 6|Set-Content (Join-Path $out 'interaction-results.json') -Encoding utf8
}
