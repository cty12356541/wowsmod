param([switch]$Check)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$frontendRoot = Join-Path $repoRoot 'preview_app\web'
$distRoot = Join-Path $frontendRoot 'dist'
$embeddedRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'crates\wowsmod\preview_web'))
$manifestPath = Join-Path $embeddedRoot 'build-info.json'

function Get-RelativeName([string]$Base, [string]$File) {
    $prefix = [IO.Path]::GetFullPath($Base).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $full = [IO.Path]::GetFullPath($File)
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path is outside its expected tree: $full"
    }
    return $full.Substring($prefix.Length).Replace('\', '/')
}

function Get-SourceFingerprint {
    $files = @()
    foreach ($dir in @('src', 'scripts')) {
        $files += Get-ChildItem -LiteralPath (Join-Path $frontendRoot $dir) -File -Recurse |
            Where-Object { $_.FullName -notmatch '[\\/]\.agentdash[\\/]' }
    }
    foreach ($name in @('package.json', 'package-lock.json', 'index.html', 'tsconfig.json', 'vite.config.ts')) {
        $files += Get-Item -LiteralPath (Join-Path $frontendRoot $name)
    }
    $entries = @($files | Sort-Object FullName | ForEach-Object {
        [ordered]@{ path = Get-RelativeName $frontendRoot $_.FullName; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    $bytes = [Text.Encoding]::UTF8.GetBytes(($entries | ConvertTo-Json -Depth 3 -Compress))
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose() }
}

$sourceHash = Get-SourceFingerprint
if ($Check) {
    if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'Embedded preview has no build-info.json; run scripts/build_preview.ps1 first.' }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.schema -ne 1 -or $manifest.source_sha256 -ne $sourceHash) {
        throw 'Frontend source differs from the embedded build. Run scripts/build_preview.ps1.'
    }
    foreach ($file in $manifest.files) {
        $target = [IO.Path]::GetFullPath((Join-Path $embeddedRoot $file.path))
        $null = Get-RelativeName $embeddedRoot $target
        if (-not (Test-Path -LiteralPath $target -PathType Leaf)) { throw "Embedded file is missing: $($file.path)" }
        if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
            throw "Embedded file differs from its build manifest: $($file.path)"
        }
    }
    $expected = @($manifest.files | ForEach-Object { $_.path }) + 'build-info.json'
    $extra = @(Get-ChildItem -LiteralPath $embeddedRoot -File -Recurse | Where-Object { (Get-RelativeName $embeddedRoot $_.FullName) -notin $expected })
    if ($extra.Count -gt 0) { throw "Unexpected embedded files: $($extra.Name -join ', ')" }
    Write-Output "Preview build matches source $sourceHash ($($manifest.files.Count) files)."
    exit 0
}

Push-Location $frontendRoot
try {
    & npm.cmd run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed; embedded files were not changed.' }
} finally { Pop-Location }
if (-not (Test-Path -LiteralPath (Join-Path $distRoot 'index.html'))) { throw 'No dist/index.html. Build the frontend first.' }
if ((Get-SourceFingerprint) -ne $sourceHash) { throw 'Frontend source changed during the build; retry after edits finish.' }

$bundleFiles = @(Get-ChildItem -LiteralPath $distRoot -File -Recurse | Sort-Object FullName | ForEach-Object {
    [ordered]@{ path = Get-RelativeName $distRoot $_.FullName; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
})
$oldFiles = @()
if (Test-Path -LiteralPath $manifestPath) {
    $old = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $oldFiles = @($old.files | ForEach-Object { $_.path })
}
foreach ($file in $bundleFiles) {
    $target = [IO.Path]::GetFullPath((Join-Path $embeddedRoot $file.path))
    $null = Get-RelativeName $embeddedRoot $target
    New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($target)) -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $distRoot $file.path) -Destination $target -Force
}
foreach ($oldFile in $oldFiles) {
    if ($oldFile -notin @($bundleFiles | ForEach-Object { $_.path })) {
        $target = [IO.Path]::GetFullPath((Join-Path $embeddedRoot $oldFile))
        $null = Get-RelativeName $embeddedRoot $target
        if (Test-Path -LiteralPath $target -PathType Leaf) { Remove-Item -LiteralPath $target }
    }
}
$revision = (& git -C $repoRoot rev-parse HEAD).Trim()
$manifest = [ordered]@{ schema = 1; frontend_baseline = '1a1fba8'; source_git_commit = $revision; source_sha256 = $sourceHash; files = $bundleFiles }
$manifestJson = ($manifest | ConvertTo-Json -Depth 5).Replace("`r`n", "`n") + "`n"
[IO.File]::WriteAllText($manifestPath, $manifestJson, [Text.UTF8Encoding]::new($false))
Write-Output "Embedded $($bundleFiles.Count) preview files from source $sourceHash."
