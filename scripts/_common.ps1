# 验收脚本公共部分。所有输出写到 <仓库>\out\ 下(已在 .gitignore 中)。
# 只读游戏目录; 不写 D:\Korabli, 不启动游戏。
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$OutputEncoding = [Text.Encoding]::UTF8
$Script:Root = Split-Path -Parent $PSScriptRoot

function Get-WowsmodExe([string]$Exe) {
    if ($Exe) { return (Resolve-Path $Exe).Path }
    $p = Join-Path $Script:Root 'target\release\wowsmod.exe'
    if (-not (Test-Path $p)) { throw "找不到 $p ; 先在仓库根目录运行 cargo build --release" }
    return $p
}

function Get-PfsunpackExe([string]$P) {
    if ($P) { return (Resolve-Path $P).Path }
    if ($env:PFSUNPACK) { return $env:PFSUNPACK }
    $d = '<WORK_DIR>\tools\mkut\pfsunpack.exe'
    if (Test-Path $d) { return $d }
    throw '找不到 pfsunpack.exe(MKUT): 用 -Pfsunpack 参数或环境变量 PFSUNPACK 指定'
}

# 运行 wowsmod --json <args>, 返回 @{ Code; Json; Raw }
function Invoke-WowsmodJson([string]$Exe, [string[]]$Arguments) {
    $raw = & $Exe --json @Arguments 2>$null
    $code = $LASTEXITCODE
    $txt = ($raw -join "`n")
    $json = $null
    if ($txt.Trim()) { $json = $txt | ConvertFrom-Json }
    return [pscustomobject]@{ Code = $code; Json = $json; Raw = $txt }
}

function Get-GameInfo([string]$Exe, [string]$Game) {
    $a = @('doctor'); if ($Game) { $a = @('--game', $Game, 'doctor') }
    $r = Invoke-WowsmodJson $Exe $a
    if (-not $r.Json.game) { throw "wowsmod doctor 没有找到游戏客户端: $($r.Raw)" }
    return $r.Json.game
}

function New-CleanDir([string]$Path) {
    # 只清理本仓库 out\ 之下的目录, 防止误删
    $full = [IO.Path]::GetFullPath($Path)
    $outRoot = [IO.Path]::GetFullPath((Join-Path $Script:Root 'out'))
    if (-not $full.StartsWith($outRoot, [StringComparison]::OrdinalIgnoreCase)) { throw "拒绝清理 out\ 以外的目录: $full" }
    if (Test-Path $full) { Remove-Item -Recurse -Force $full }
    New-Item -ItemType Directory -Force $full | Out-Null
    return $full
}

function Get-Sha256([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash
}

# 用 pfsunpack 解出精确路径列表(分批, 每批若干个 -I)
function Invoke-PfsunpackList([string]$Pfs, $GameInfo, [string[]]$Paths, [string]$OutDir, [int]$Batch = 40) {
    for ($i = 0; $i -lt $Paths.Count; $i += $Batch) {
        $chunk = $Paths[$i..([Math]::Min($i + $Batch, $Paths.Count) - 1)]
        $a = @($GameInfo.idx_dir, '-p', $GameInfo.res_packages, '-x', '-o', $OutDir)
        foreach ($p in $chunk) { $a += @('-I', $p) }
        & $Pfs @a 2>&1 | Out-Null
    }
}

function Join-VPath([string]$Base, [string]$VPath) { return (Join-Path $Base ($VPath -replace '/', '\')) }
