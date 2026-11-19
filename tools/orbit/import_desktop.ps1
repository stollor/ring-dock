# Resolves desktop shortcuts to their real targets and updates only the supplied dock config.
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

function Get-ItemKey([string]$target, [string]$args, [string]$workdir) {
    return ($target.ToLowerInvariant() + '|' + $args + '|' + $workdir.ToLowerInvariant())
}
function Read-InternetUrl([string]$path) {
    $text=[IO.File]::ReadAllText($path)
    foreach($line in ($text -split '\r?\n')){
        if($line -match '^\s*URL=(.*)$'){
            $url=$Matches[1].Trim()
            if($url){return $url}
        }
    }
    return $null
}
function Get-TargetKind([string]$target) {
    $lower=$target.ToLowerInvariant()
    if($lower.StartsWith('http://') -or $lower.StartsWith('https://')){return 'url'}
    $colon=$lower.IndexOf(':')
    if($colon -gt 1){
        $rest=$lower.Substring($colon+1)
        if(-not $rest.StartsWith('\') -and -not $rest.StartsWith('/')){return 'url'}
    }
    if([IO.Directory]::Exists($target)){return 'folder'}
    foreach($ext in '.lnk','.exe','.bat','.cmd'){if($lower.EndsWith($ext)){return 'program'}}
    return 'file'
}
function Resolve-ShortcutFile($shell, [string]$path) {
    if(-not (Test-Path -LiteralPath $path -PathType Leaf)){return $null}
    $ext=[IO.Path]::GetExtension($path).ToLowerInvariant()
    if($ext -eq '.lnk'){
        $link=$shell.CreateShortcut($path)
        try {
            $target=[Environment]::ExpandEnvironmentVariables([string]$link.TargetPath).Trim()
            if(-not $target){return $null}
            return [pscustomobject]@{target=$target;args=[string]$link.Arguments;workdir=[string]$link.WorkingDirectory;kind=(Get-TargetKind $target)}
        } finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($link) }
    }
    if($ext -in '.url','.website'){
        $url=Read-InternetUrl $path
        if(-not $url){return $null}
        return [pscustomobject]@{target=$url;args='';workdir='';kind='url'}
    }
    return $null
}
function Set-LinkField($obj, [string]$name, [string]$value) {
    $has=$obj.PSObject.Properties.Name -contains $name
    if([string]::IsNullOrEmpty($value)){
        if($has){$obj.PSObject.Properties.Remove($name)}
        return
    }
    if($has){$obj.$name=$value}else{$obj | Add-Member -NotePropertyName $name -NotePropertyValue $value}
}

$paths=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$found=[Collections.Generic.List[object]]::new()
$skippedDirectories=[Collections.Generic.List[string]]::new()
$retargeted=0
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
                $resolved=Resolve-ShortcutFile $shell $entry.FullName
                if($resolved){
                    $found.Add([pscustomobject]@{name=$entry.BaseName;kind=$resolved.kind;target=$resolved.target;args=$resolved.args;workdir=$resolved.workdir;source=$entry.FullName;sha256=(Get-FileHash -LiteralPath $entry.FullName).Hash})
                } else {
                    $kind= if($entry.Extension.ToLowerInvariant() -in '.url','.website'){'url'}else{'program'}
                    $found.Add([pscustomobject]@{name=$entry.BaseName;kind=$kind;target=$entry.FullName;args='';workdir='';source=$entry.FullName;sha256=(Get-FileHash -LiteralPath $entry.FullName).Hash})
                }
            }
        }
    }
    foreach($q in $cfg.quadrants){
        foreach($i in @($q.items)){
            $resolved=Resolve-ShortcutFile $shell ([string]$i.target)
            if(-not $resolved){continue}
            $same=([string]$i.target).Equals($resolved.target,[StringComparison]::OrdinalIgnoreCase) -and ([string]$i.args) -eq [string]$resolved.args -and ([string]$i.workdir).Equals([string]$resolved.workdir,[StringComparison]::OrdinalIgnoreCase) -and [string]$i.kind -eq $resolved.kind
            if($same){continue}
            $i.target=$resolved.target
            $i.kind=$resolved.kind
            Set-LinkField $i 'args' ([string]$resolved.args)
            Set-LinkField $i 'workdir' ([string]$resolved.workdir)
            $retargeted++
        }
    }
} finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell) }

$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach($q in $cfg.quadrants){foreach($i in $q.items){[void]$seen.Add((Get-ItemKey ([string]$i.target) ([string]$i.args) ([string]$i.workdir)))}}
$added=[Collections.Generic.List[object]]::new();$duplicates=[Collections.Generic.List[string]]::new()
# 一个来源目录对应多个候选分类名：优先新六维分类，其次兼容旧配置里的分类名。
$categoryMap=@{
    '工具效率'=@('系统环境','日常协作');
    '通讯社交'=@('沟通','日常协作');
    '开发编程'=@('写码','开发创作');
    '设计引擎'=@('造画面','开发创作');
    'AI应用'=@('问 AI','AI助手');
    '游戏影音'=@('玩与看','娱乐影音')
}
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
    if(-not $seen.Add((Get-ItemKey $item.target $item.args $item.workdir))){$duplicates.Add($item.target);continue}
    $destination=@()
    foreach($candidate in @(Get-SourceCategory $item.source)){
        if(-not $candidate){continue}
        $matched=@($cfg.quadrants | Where-Object label -eq $candidate | Select-Object -First 1)
        if($matched.Count -gt 0){$destination=$matched;break}
    }
    if($destination.Count -eq 0){$destination=@($cfg.quadrants | Where-Object kind -eq $item.kind | Select-Object -First 1)}
    if($destination.Count -eq 0){$destination=@($cfg.quadrants | Select-Object -First 1)}
    $q=$destination[0]
    $entry=[ordered]@{name=$item.name;kind=$item.kind;target=$item.target}
    if($item.args){$entry.args=$item.args}
    if($item.workdir){$entry.workdir=$item.workdir}
    $q.items=@($q.items)+@([pscustomobject]$entry)
    $added.Add([pscustomobject]@{name=$item.name;kind=$item.kind;target=$item.target;category=$q.label})
}
$backup=$null
if(-not $ScanOnly -and ($added.Count -gt 0 -or $retargeted -gt 0)){
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
$report=[ordered]@{date=(Get-Date).ToString('o');roots=$Roots;scanOnly=[bool]$ScanOnly;discovered=$found.Count;added=$added.Count;retargeted=$retargeted;duplicates=$duplicates.Count;skippedReparseDirectories=$skippedDirectories.ToArray();configBeforeSha256=$beforeHash;configAfterSha256=(Get-FileHash -LiteralPath $ConfigPath).Hash;backup=$backup;items=$added.ToArray();sources=$found.ToArray();sourceFilesModified=$false;shortcutsExecuted=$false}
if($ReportPath){$ReportPath=[IO.Path]::GetFullPath($ReportPath);[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($ReportPath))|Out-Null;[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))}
$report | ConvertTo-Json -Depth 12
