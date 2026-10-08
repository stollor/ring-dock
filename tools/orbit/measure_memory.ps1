param([string]$Exe, [string]$Output, [int]$Cycles=3, [int]$ExistingProcessId=0)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'control.ps1')
[OrbitControl]::SetThreadDpiAwarenessContext([IntPtr](-4)) | Out-Null
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DockMemory {
 [DllImport("psapi.dll")] static extern bool QueryWorkingSet(IntPtr h,IntPtr buffer,int size);
 [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr h,uint flag);
 public struct RECT{public int l,t,r,b;}
 [DllImport("user32.dll")] public static extern bool SystemParametersInfoW(uint a,uint b,out RECT r,uint f);
 public static long PrivateWorkingSet(IntPtr h){
  int size=4*1024*1024;IntPtr buffer=Marshal.AllocHGlobal(size);
  try{
   if(!QueryWorkingSet(h,buffer,size))throw new Exception("QueryWorkingSet failed");
   long pages=Marshal.ReadIntPtr(buffer).ToInt64(),count=0;
   if(pages>(size/IntPtr.Size)-1)throw new Exception("Working-set buffer too small");
   for(long i=0;i<pages;i++)if((Marshal.ReadIntPtr(buffer,(int)((i+1)*IntPtr.Size)).ToInt64()&256)==0)count++;
   return count*4096;
  }finally{Marshal.FreeHGlobal(buffer);}
 }
}
'@
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$cfgPath=Join-Path $root 'target/release/config.json'
$cfg=Get-Content $cfgPath -Raw | ConvertFrom-Json
$rect=New-Object DockMemory+RECT
[DockMemory]::SystemParametersInfoW(48,0,[ref]$rect,0) | Out-Null
$cx=$cfg.dock_position.x*($rect.r-$rect.l)
$cy=$cfg.dock_position.y*($rect.b-$rect.t)
$rows=[Collections.Generic.List[object]]::new()
function Sample($phase){
 $p.Refresh()
 [pscustomobject]@{phase=$phase;time=(Get-Date -Format o);privateWorkingSetMiB=[math]::Round([DockMemory]::PrivateWorkingSet($p.Handle)/1MB,3);privateCommitMiB=[math]::Round($p.PrivateMemorySize64/1MB,3);workingSetMiB=[math]::Round($p.WorkingSet64/1MB,3);handles=$p.HandleCount;threads=$p.Threads.Count;gdi=[DockMemory]::GetGuiResources($p.Handle,0);user=[DockMemory]::GetGuiResources($p.Handle,1)}
}
function Settle($phase){
 $last=$null;$stable=0;$deadline=(Get-Date).AddSeconds(25)
 do{
  $now=Sample $phase
  if($last -and [math]::Abs($now.privateCommitMiB-$last.privateCommitMiB) -lt .1 -and $now.threads -eq $last.threads -and $now.handles -eq $last.handles){$stable++}else{$stable=0}
  $last=$now
  if($stable -ge 3){break}
  Start-Sleep -Milliseconds 500
 }while((Get-Date) -lt $deadline)
 $rows.Add($now);$now | ConvertTo-Json -Compress | Write-Output
}
$priorCfg=$env:RING_DOCK_CONFIG;$priorFrame=$env:RING_DOCK_FRAME
try{
 $env:RING_DOCK_CONFIG=$cfgPath;$env:RING_DOCK_FRAME=$null
 if($ExistingProcessId){$p=Get-Process -Id $ExistingProcessId}else{
  $p=Start-Process $Exe -ArgumentList '--preview' -PassThru -WindowStyle Hidden
 }
 $env:RING_DOCK_CONFIG=$priorCfg;$env:RING_DOCK_FRAME=$priorFrame
 $deadline=(Get-Date).AddSeconds(15)
 do{$h=[OrbitControl]::Find($p.Id);if($h -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}while((Get-Date) -lt $deadline)
 if($h -eq [IntPtr]::Zero){throw 'No benchmark window'}
 Settle 'cold-collapsed'
 for($cycle=1;$cycle -le $Cycles;$cycle++){
  for($q=0;$q -lt $cfg.quadrant_count;$q++){
   $angle=(-45+$q*360/$cfg.quadrant_count)*[math]::PI/180
   [OrbitControl]::Click($h,[int]($cx+136*[math]::Cos($angle)),[int]($cy+136*[math]::Sin($angle)))
   if([OrbitControl]::Title($h) -ne "ring-dock#expanded=$q"){throw 'Category did not expand'}
   Settle "cycle-$cycle-category-$q"
  }
  [OrbitControl]::Click($h,[int]$cx,[int]$cy)
  Settle "cycle-$cycle-collapsed"
 }
 $modules=@($p.Modules | Where-Object {$_.ModuleName -match 'ShellExt|d2d|dwrite|Warp'} | Select-Object ModuleName)
 @{exe=$Exe;configHash=(Get-FileHash $cfgPath).Hash;cycles=$Cycles;diagnosticFrames=$false;measurements=$rows.ToArray();modules=$modules} | ConvertTo-Json -Depth 6 | Set-Content $Output -Encoding utf8
}finally{
 $env:RING_DOCK_CONFIG=$priorCfg;$env:RING_DOCK_FRAME=$priorFrame
 if(-not $ExistingProcessId -and $p -and -not $p.HasExited){[OrbitControl]::Message($h,0x111,103,0);if(-not $p.WaitForExit(4000)){Stop-Process -Id $p.Id}}
}
