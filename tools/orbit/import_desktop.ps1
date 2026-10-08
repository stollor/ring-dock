# Read-only shortcut discovery; only the explicitly supplied dock config is updated.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ConfigPath,
    [string[]]$Roots=@([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('CommonDesktopDirectory')),
    [string]$ReportPath='',
    [switch]$ScanOnly
)
$ErrorActionPreference='Stop'
$ConfigPath=(Resolve-Path -LiteralPath $ConfigPath).Path
$beforeHash=(Get-FileHash -LiteralPath $ConfigPath -Algorithm SHA256).Hash
$cfg=Get-Content -LiteralPath $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
if(-not $cfg.quadrants -or $cfg.quadrants.Count -ne $cfg.quadrant_count){throw 'Invalid quadrant configuration; nothing written.'}
$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach($q in $cfg.quadrants){foreach($i in $q.items){[void]$seen.Add([string]$i.target)}}
$paths=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$found=[Collections.Generic.List[object]]::new()
$skippedDirectories=[Collections.Generic.List[string]]::new()
$shell=New-Object -ComObject WScript.Shell
try {
    foreach($root in $Roots){
        $root=(Resolve-Path -LiteralPath $root).Path
        $pending=[Collections.Generic.Stack[string]]::new();$pending.Push($root)
        while($pending.Count -gt 0){
            $dir=$pending.Pop()
            foreach($entry in (Get-ChildItem -LiteralPath $dir -Force | Sort-Object Name)){
                if($entry.PSIsContainer){
                    # Don't escape the requested desktop tree or loop through junctions.
                    if(($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0){$skippedDirectories.Add($entry.FullName)}else{$pending.Push($entry.FullName)}
                    continue
                }
                if($entry.Extension.ToLowerInvariant() -notin '.lnk','.url','.website','.appref-ms'){continue}
                if(-not $paths.Add($entry.FullName)){continue}
                $kind='program'
                if($entry.Extension.ToLowerInvariant() -in '.url','.website'){$kind='url'}
                elseif($entry.Extension -ieq '.lnk'){
                    $link=$shell.CreateShortcut($entry.FullName)
                    try {
                        $target=[Environment]::ExpandEnvironmentVariables($link.TargetPath)
                        if($target -and [IO.Directory]::Exists($target)){$kind='folder'}
                        elseif($target -and [IO.File]::Exists($target) -and [IO.Path]::GetExtension($target).ToLowerInvariant() -notin '.exe','.com','.bat','.cmd','.ps1'){$kind='file'}
                    } finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($link) }
                }
                $found.Add([pscustomobject]@{name=$entry.BaseName;kind=$kind;target=$entry.FullName;sha256=(Get-FileHash -LiteralPath $entry.FullName).Hash})
            }
        }
    }
} finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell) }
$added=[Collections.Generic.List[object]]::new();$duplicates=[Collections.Generic.List[string]]::new()
$categoryMap=@{'工具效率'='日常协作';'通讯社交'='日常协作';'开发编程'='开发创作';'设计引擎'='开发创作';'AI应用'='AI助手';'游戏影音'='娱乐影音'}
$normRoots=@(foreach($r in $Roots){try{[IO.Path]::GetFullPath($r).TrimEnd([IO.Path]::DirectorySeparatorChar)}catch{$r}})
function Get-SourceCategory([string]$shortcutPath){
    $dir=[IO.Path]::GetDirectoryName($shortcutPath)
    while($dir){
        $name=[IO.Path]::GetFileName($dir)
        if($categoryMap.ContainsKey($name)){return $categoryMap[$name]}
        $isRoot=$false
        foreach($r in $normRoots){if([string]::Equals($dir.TrimEnd([IO.Path]::DirectorySeparatorChar),$r,[StringComparison]::OrdinalIgnoreCase)){$isRoot=$true;break}}
        if($isRoot){return $null}
        $parent=[IO.Path]::GetDirectoryName($dir)
        if(-not $parent -or $parent -eq $dir){return $null}
        $dir=$parent
    }
    return $null
}
foreach($item in ($found | Sort-Object target)){
    if(-not $seen.Add($item.target)){$duplicates.Add($item.target);continue}
    $destination=@()
    $sourceCategory=Get-SourceCategory $item.target
    if($sourceCategory){$destination=@($cfg.quadrants | Where-Object label -eq $sourceCategory | Select-Object -First 1)}
    if($destination.Count -eq 0){$destination=@($cfg.quadrants | Where-Object kind -eq $item.kind | Select-Object -First 1)}
    if($destination.Count -eq 0){$destination=@($cfg.quadrants | Select-Object -First 1)}
    $q=$destination[0]
    $q.items=@($q.items)+@([pscustomobject]@{name=$item.name;kind=$item.kind;target=$item.target})
    $added.Add([pscustomobject]@{name=$item.name;kind=$item.kind;target=$item.target;category=$q.label})
}
$backup=$null
if(-not $ScanOnly -and $added.Count -gt 0){
    $text=$cfg | ConvertTo-Json -Depth 32
    # Validate before touching the original. File.Replace is atomic on this volume.
    $validated=$text | ConvertFrom-Json
    if($validated.quadrants.Count -ne $cfg.quadrants.Count){throw 'Roundtrip failed'}
    $temp=Join-Path ([IO.Path]::GetDirectoryName($ConfigPath)) ('.desktop-import.'+[Guid]::NewGuid().ToString('N')+'.tmp')
    $backup=$ConfigPath+'.desktop-import.'+(Get-Date -Format 'yyyyMMdd-HHmmss-fff')+'.bak'
    try {
        $bytes=[Text.UTF8Encoding]::new($false).GetBytes($text)
        $stream=[IO.File]::Open($temp,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try{$stream.Write($bytes,0,$bytes.Length);$stream.Flush($true)}finally{$stream.Dispose()}
        if((Get-FileHash -LiteralPath $ConfigPath).Hash -ne $beforeHash){throw 'Config changed during scan; import aborted to preserve concurrent edits.'}
        [IO.File]::Replace($temp,$ConfigPath,$backup,$false)
    } finally {if(Test-Path -LiteralPath $temp){Remove-Item -LiteralPath $temp}}
}
$report=[ordered]@{date=(Get-Date).ToString('o');roots=$Roots;scanOnly=[bool]$ScanOnly;discovered=$found.Count;added=$added.Count;duplicates=$duplicates.Count;skippedReparseDirectories=$skippedDirectories.ToArray();configBeforeSha256=$beforeHash;configAfterSha256=(Get-FileHash -LiteralPath $ConfigPath).Hash;backup=$backup;items=$added.ToArray();sources=$found.ToArray();sourceFilesModified=$false;shortcutsExecuted=$false}
if($ReportPath){$ReportPath=[IO.Path]::GetFullPath($ReportPath);[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($ReportPath))|Out-Null;[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))}
$report | ConvertTo-Json -Depth 12
