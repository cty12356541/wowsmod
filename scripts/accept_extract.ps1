<#
验收 M1-3: wowsmod extract 与 pfsunpack(MKUT)逐字节一致。
  1) 固定种子随机抽取 >= 200 个二进制资源(dds/dd0-2/geometry/splash/wem/bnk/...), 外加 10 个最大的多块 Oodle 条目;
  2) 整船: 勃艮第 PFSB510(模型 FSB025_Bourgogne_1945)与黎塞留 PFSB108(FSB003_Richelieu_1944)全部文件。
两边各自解包到 out\accept_extract\{wowsmod,pfsunpack}\, 逐个比较 SHA-256。
结果: out\accept_extract\report.json 与 files.csv; 全部一致时退出码 0。
用法: powershell -ExecutionPolicy Bypass -File scripts\accept_extract.ps1 [-Count 240] [-Seed 20261010] [-Game D:\Korabli]
#>
param([int]$Count = 240, [int]$Seed = 20261010, [string]$Game, [string]$Exe, [string]$Pfsunpack, [string[]]$Ships = @('PFSB510', 'PFSB108'))
. "$PSScriptRoot\_common.ps1"
$wm = Get-WowsmodExe $Exe
$pfs = Get-PfsunpackExe $Pfsunpack
$gi = Get-GameInfo $wm $Game
$gargs = @(); if ($Game) { $gargs = @('--game', $Game) }
$base = New-CleanDir (Join-Path $Root 'out\accept_extract')
Write-Host "游戏: $($gi.root)  idx: $($gi.idx_dir)"

# ---- 1) 随机样本池
$exts = @('dds', 'dd0', 'dd1', 'dd2', 'geometry', 'splash', 'wem', 'bnk', 'png', 'jpg', 'bin', 'data', 'swf', 'ttf', 'otf', 'fsb', 'anim', 'animation', 'mov', 'usm')
$pool = @()
foreach ($e in $exts) {
    $r = Invoke-WowsmodJson $wm ($gargs + @('find', "*.$e"))
    if ($r.Json -and $r.Json.files) { $pool += $r.Json.files }
}
$pool = $pool | Sort-Object path -Unique
Write-Host "二进制资源池: $($pool.Count) 个文件"
$rng = New-Object System.Random $Seed
$picked = New-Object System.Collections.Generic.List[object]
$used = @{}
while ($picked.Count -lt [Math]::Min($Count, $pool.Count)) {
    $f = $pool[$rng.Next($pool.Count)]
    if (-not $used.ContainsKey($f.path)) { $used[$f.path] = 1; $picked.Add($f) }
}
# 最大的 10 个 Oodle 条目(必然是多块容器, 覆盖 M0 未处理的情形)
$big = $pool | Where-Object { $_.compression -eq 'LestaOodle' -and -not $used.ContainsKey($_.path) } | Sort-Object { [int64]$_.size } -Descending | Select-Object -First 10
foreach ($f in $big) { $used[$f.path] = 1; $picked.Add($f) }
$randomPaths = @($picked | ForEach-Object { $_.path })
$randomPaths | Set-Content -Encoding UTF8 (Join-Path $base 'random_list.txt')

# ---- 2) 整船
$shipSets = @{}
foreach ($s in $Ships) {
    $r = Invoke-WowsmodJson $wm ($gargs + @('extract', '--ship', $s, '--dry-run', '-o', (Join-Path $base 'dummy')))
    if ($r.Code -ne 0) { throw "extract --ship $s --dry-run 失败: $($r.Raw)" }
    $shipSets[$s] = @($r.Json.files)
    Write-Host "整船 $s : $($shipSets[$s].Count) 个文件"
}
$all = @($randomPaths) + @($shipSets.Values | ForEach-Object { $_ }) | Sort-Object -Unique
$listFile = Join-Path $base 'all_list.txt'
$all | Set-Content -Encoding UTF8 $listFile

# ---- wowsmod 解包
$wmOut = Join-Path $base 'wowsmod'
$sw = [Diagnostics.Stopwatch]::StartNew()
$r = Invoke-WowsmodJson $wm ($gargs + @('extract', '--list', $listFile, '-o', $wmOut))
$wmSec = $sw.Elapsed.TotalSeconds
if ($r.Code -ne 0) { Write-Warning "wowsmod extract 退出码 $($r.Code): $($r.Raw)" }
# ---- pfsunpack 解包
$pfsOut = Join-Path $base 'pfsunpack'
$sw = [Diagnostics.Stopwatch]::StartNew()
Invoke-PfsunpackList $pfs $gi $all $pfsOut
$pfsSec = $sw.Elapsed.TotalSeconds

# ---- 比较
$meta = @{}; foreach ($f in $pool) { $meta[$f.path] = $f }
$rows = foreach ($p in $all) {
    $a = Get-Sha256 (Join-VPath $wmOut $p)
    $b = Get-Sha256 (Join-VPath $pfsOut $p)
    $m = $meta[$p]
    [pscustomobject]@{
        path = $p; group = $(if ($randomPaths -contains $p) { 'random' } else { 'ship' })
        size = $(if ($m) { $m.size } else { (Get-Item -LiteralPath (Join-VPath $wmOut $p) -ErrorAction SilentlyContinue).Length })
        compression = $(if ($m) { $m.compression } else { '' })
        sha256_wowsmod = $a; sha256_pfsunpack = $b
        status = $(if (-not $a) { 'wowsmod 缺失' } elseif (-not $b) { 'pfsunpack 缺失' } elseif ($a -eq $b) { '一致' } else { '不一致' })
    }
}
$rows | Export-Csv -NoTypeInformation -Encoding UTF8 (Join-Path $base 'files.csv')
$ok = @($rows | Where-Object status -eq '一致').Count
$rnd = @($rows | Where-Object { $randomPaths -contains $_.path })
$summary = [ordered]@{
    seed = $Seed; pool = $pool.Count
    random_files = $randomPaths.Count
    random_identical = @($rnd | Where-Object status -eq '一致').Count
    random_by_ext = ($rnd | Group-Object { [IO.Path]::GetExtension($_.path) } | Sort-Object Count -Descending | ForEach-Object { "$($_.Name)=$($_.Count)" }) -join ' '
    random_oodle = @($rnd | Where-Object compression -eq 'LestaOodle').Count
    random_stored = @($rnd | Where-Object compression -eq 'Stored').Count
    ships = [ordered]@{}
    total_files = $all.Count; total_identical = $ok
    mismatches = @($rows | Where-Object status -ne '一致' | Select-Object path, status)
    wowsmod_seconds = [Math]::Round($wmSec, 2); pfsunpack_seconds = [Math]::Round($pfsSec, 2)
    bc7prep_decoded = $(if ($r.Json) { $r.Json.bc7prep_decoded } else { $null })
}
foreach ($s in $Ships) {
    $set = $shipSets[$s]
    $summary.ships[$s] = [ordered]@{ files = $set.Count; identical = @($rows | Where-Object { $set -contains $_.path -and $_.status -eq '一致' }).Count; bytes = ($rows | Where-Object { $set -contains $_.path } | Measure-Object size -Sum).Sum }
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 (Join-Path $base 'report.json')
$summary | ConvertTo-Json -Depth 6
if ($ok -eq $all.Count) { Write-Host "通过: $ok/$($all.Count) 个文件与 pfsunpack 逐字节一致" -ForegroundColor Green; exit 0 }
Write-Host "未通过: $ok/$($all.Count) 一致" -ForegroundColor Red; exit 1
