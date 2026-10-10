<#
补充验收(BC7Prep): 大批量贴图与 pfsunpack 逐字节一致。
  固定种子从全部 .dds/.dd0/.dd1/.dd2 中随机抽 -Count 个(默认 2000), 两边各自解包后比较 SHA-256;
  wowsmod 默认把 BC7Prep 贴图还原为标准 BC7, 报告其中有多少个是 BC7Prep。
  并用 examples\bc7_coverage 统计这些还原结果覆盖到的 BC7 模式/旋转组合(检查逆变换各分支是否都被真实数据验证过)。
结果: out\accept_textures\report.json, files.csv
用法: powershell -ExecutionPolicy Bypass -File scripts\accept_textures.ps1 [-Count 2000] [-Seed 20261010]
#>
param([int]$Count = 2000, [int]$Seed = 20261010, [string]$Game, [string]$Exe, [string]$Pfsunpack)
. "$PSScriptRoot\_common.ps1"
$wm = Get-WowsmodExe $Exe
$pfs = Get-PfsunpackExe $Pfsunpack
$gi = Get-GameInfo $wm $Game
$gargs = @(); if ($Game) { $gargs = @('--game', $Game) }
$base = New-CleanDir (Join-Path $Root 'out\accept_textures')
$pool = @()
foreach ($e in 'dds', 'dd0', 'dd1', 'dd2') {
    $r = Invoke-WowsmodJson $wm ($gargs + @('find', "*.$e"))
    if ($r.Json -and $r.Json.files) { $pool += $r.Json.files }
}
$pool = @($pool | Sort-Object path -Unique)
Write-Host "贴图池: $($pool.Count) 个文件"
$rng = New-Object System.Random $Seed
$used = @{}; $picked = New-Object System.Collections.Generic.List[string]
while ($picked.Count -lt [Math]::Min($Count, $pool.Count)) {
    $p = $pool[$rng.Next($pool.Count)].path
    if (-not $used.ContainsKey($p)) { $used[$p] = 1; $picked.Add($p) }
}
$listFile = Join-Path $base 'list.txt'
$picked | Set-Content -Encoding UTF8 $listFile
$wmOut = Join-Path $base 'wowsmod'
$sw = [Diagnostics.Stopwatch]::StartNew()
$r = Invoke-WowsmodJson $wm ($gargs + @('extract', '--list', $listFile, '-o', $wmOut))
$wmSec = $sw.Elapsed.TotalSeconds
if ($r.Code -ne 0) { Write-Warning "wowsmod extract 退出码 $($r.Code): $($r.Raw)" }
$prep = @{}; foreach ($p in @($r.Json.bc7prep_files)) { $prep[$p] = 1 }
$pfsOut = Join-Path $base 'pfsunpack'
$sw = [Diagnostics.Stopwatch]::StartNew()
Invoke-PfsunpackList $pfs $gi $picked $pfsOut
$pfsSec = $sw.Elapsed.TotalSeconds
$rows = foreach ($p in $picked) {
    $a = Get-Sha256 (Join-VPath $wmOut $p); $b = Get-Sha256 (Join-VPath $pfsOut $p)
    [pscustomobject]@{ path = $p; bc7prep = [bool]$prep[$p]; sha256_wowsmod = $a; sha256_pfsunpack = $b
        status = $(if (-not $a) { 'wowsmod 缺失' } elseif (-not $b) { 'pfsunpack 缺失' } elseif ($a -eq $b) { '一致' } else { '不一致' }) }
}
$rows | Export-Csv -NoTypeInformation -Encoding UTF8 (Join-Path $base 'files.csv')
$prepList = Join-Path $base 'bc7prep_decoded.txt'
@($rows | Where-Object bc7prep | ForEach-Object { Join-VPath $wmOut $_.path }) | Set-Content -Encoding UTF8 $prepList
$cov = $null
$covExe = Join-Path $Root 'target\release\examples\bc7_coverage.exe'
if (Test-Path $covExe) { $cov = (& $covExe $prepList) -join "`n" | ConvertFrom-Json } else { Write-Warning "缺少 $covExe(cargo build --release --examples), 跳过覆盖统计" }
$ok = @($rows | Where-Object status -eq '一致').Count
$summary = [ordered]@{
    seed = $Seed; pool = $pool.Count; files = $picked.Count; identical = $ok
    bc7prep_files = $prep.Count; bc7prep_identical = @($rows | Where-Object { $_.bc7prep -and $_.status -eq '一致' }).Count
    mismatches = @($rows | Where-Object status -ne '一致' | Select-Object path, bc7prep, status)
    coverage = $cov
    wowsmod_seconds = [Math]::Round($wmSec, 2); pfsunpack_seconds = [Math]::Round($pfsSec, 2)
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 (Join-Path $base 'report.json')
$summary | ConvertTo-Json -Depth 6
if ($ok -eq $picked.Count) { Write-Host "通过: $ok/$($picked.Count) 个贴图与 pfsunpack 逐字节一致(其中 BC7Prep $($prep.Count) 个)" -ForegroundColor Green; exit 0 }
Write-Host "未通过: $ok/$($picked.Count) 一致" -ForegroundColor Red; exit 1
