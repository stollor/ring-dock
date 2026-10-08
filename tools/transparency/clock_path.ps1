$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out=Join-Path $root 'target\transparency-probe'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$csc=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$exe=Join-Path $out 'clock-path.exe'
& $csc /nologo /target:exe /r:System.Drawing.dll /r:System.Web.Extensions.dll "/out:$exe" (Join-Path $PSScriptRoot 'clock_path.cs')
if($LASTEXITCODE -ne 0){throw 'Compile failed'}
& $exe $out
if($LASTEXITCODE -ne 0){throw 'Probe failed'}
