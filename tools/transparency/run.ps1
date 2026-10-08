# Reproducible A/B test. Leaves ring-dock.exe/config unchanged; restores ancestor styles.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out = Join-Path $root 'target\transparency-probe'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$source = Join-Path $PSScriptRoot 'probe.cs'
$manifest = Join-Path $PSScriptRoot 'probe.manifest'
$allResults=@()
foreach ($mode in @('legacy', 'compatible')) {
  $exe = Join-Path $out "$mode.exe"
  $manifestArg = if ($mode -eq 'legacy') { '/nowin32manifest' } else { "/win32manifest:$manifest" }
  & $csc /nologo /target:exe /platform:x64 /r:System.Drawing.dll /r:System.Web.Extensions.dll "/out:$exe" $manifestArg $source
  if ($LASTEXITCODE -ne 0) { throw "Compile failed: $mode" }
  & $exe $out $mode | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "Probe failed: $mode" }
  $result = Get-Content -Raw (Join-Path $out "$mode.json") | ConvertFrom-Json
  $allResults += $result
  "=== $mode ($($result.os)) ==="
  foreach ($row in $result.rows) {
    '{0}: created={1} parent={2} present={3} error={4} capture={5}' -f $row.mode,$row.created,$row.parentOK,$row.present,$row.presentError,$row.captureControlPass
    $row.samples | Format-Table alpha,actual,expected,maxError,pixelPass,hitIsProbe -AutoSize
  }
}

$pixelChecks=0; $hitChecks=0; $inconclusive=@()
foreach($result in $allResults){
  foreach($row in $result.rows){
    if($row.mode -eq 'own-parent-direct-child' -and $result.label -eq 'legacy'){
      if($row.created){throw 'Legacy direct-child gate changed: inspect results'}
      continue
    }
    if($row.mode -eq 'desktop-Progman'){
      if(-not $row.captureControlPass){$inconclusive += "$($result.label): Progman capture control not visible";continue}
    }
    if($row.mode -like 'desktop-*' -and $row.mode -notlike 'desktop-live-*' -and $row.mode -ne 'desktop-ring-prototype'){
      if(-not $row.parentOK -or -not $row.present -or -not $row.captureControlPass){throw "Desktop control failed: $($row.mode)"}
    }
    if($row.mode -eq 'desktop-ring-prototype'){
      if(-not $row.parentOK -or -not $row.present -or $row.thumbnailHR -ne 0){throw 'Prototype presentation failed'}
    }
    foreach($sample in $row.samples){
      $pixelChecks++
      if(-not $sample.pixelPass){throw "Pixel mismatch: $($result.label)/$($row.mode)/alpha=$($sample.alpha)"}
      if($null -ne $sample.hitIsProbe){
        $hitChecks++
        if($sample.hitIsProbe -ne ($sample.alpha -gt 0)){throw 'WindowFromPoint alpha hit-test mismatch'}
      }
    }
  }
}
$summary=[ordered]@{date='2026-10-03';pixelChecks=$pixelChecks;pixelChecksPassed=$pixelChecks;hitTestChecks=$hitChecks;hitTestChecksPassed=$hitChecks;inconclusive=$inconclusive;realMouseClicksTested=$false;productionModified=$false}
$summary | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 (Join-Path $out 'summary.json')
"PASS: $pixelChecks pixel checks, $hitChecks WindowFromPoint checks. Progman control failure is inconclusive, not a no-alpha verdict."
