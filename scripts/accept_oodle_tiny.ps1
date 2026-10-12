<#
M2 任务 0 验收: Hydra/Kraken 极小块(压缩后很小的块)解压与 pfsunpack 逐字节一致。
  输入: out\m2_oodle_scan\tiny_blocks.tsv(examples\oodle_tiny_blocks 输出)。
  选取: 全部"最小块 <= -MaxSmall 字节"的条目 + 固定种子随机抽 -Random 个其它小块条目。
  两边解包(wowsmod 用 --raw, 与 pfsunpack 的 BC7Prep 还原无关的原始字节比较不公平, 故 wowsmod 用默认模式 = 与 pfsunpack 一样还原 BC7Prep)后比较 SHA-256。
结果: out\m2_oodle_scan\accept_tiny\report.json, files.csv
#>
param([int]$MaxSmall = 32, [int]$Random = 60, [int]$Seed = 20261010, [string]$Game, [string]$Exe, [string]$Pfsunpack)
. "$PSScriptRoot\_common.ps1"
$wm = Get-WowsmodExe $Exe
$pfs = Get-PfsunpackExe $Pfsunpack
$gi = Get-GameInfo $wm $Game
$gargs = @(); if ($Game) { $gargs = @('--game', $Game) }
$tsv = Join-Path $Root 'out\m2_oodle_scan\tiny_blocks.tsv'
if (-not (Test-Path $tsv)) { throw "缺少 $tsv; 先运行 target\release\examples\oodle_tiny_blocks.exe <idx> <res_packages> 64 > 该文件" }
$base = New-CleanDir (Join-Path $Root 'out\m2_oodle_scan\accept_tiny')
$min = @{}
foreach ($line in Get-Content -Encoding UTF8 $tsv) {
    $f = $line -split "`t"; if ($f.Count -lt 4) { continue }
    $c = [int]$f[2]
    if (-not $min.ContainsKey($f[0]) -or $c -lt $min[$f[0]]) { $min[$f[0]] = $c }
}
$small = @($min.Keys | Where-Object { $min[$_] -le $MaxSmall } | Sort-Object)
$rest = @($min.Keys | Where-Object { $min[$_] -gt $MaxSmall } | Sort-Object)
$rng = New-Object System.Random $Seed
$pickRest = New-Object System.Collections.Generic.List[string]; $used = @{}
while ($pickRest.Count -lt [Math]::Min($Random, $rest.Count)) {
    $p = $rest[$rng.Next($rest.Count)]; if (-not $used.ContainsKey($p)) { $used[$p] = 1; $pickRest.Add($p) }
}
$picked = @($small) + @($pickRest)
Write-Host "含小块条目 $($min.Count) 个; 其中最小块 <= $MaxSmall 字节 $($small.Count) 个, 另随机 $($pickRest.Count) 个"
$listFile = Join-Path $base 'list.txt'
$picked | Set-Content -Encoding UTF8 $listFile
$wmOut = Join-Path $base 'wowsmod'
$r = Invoke-WowsmodJson $wm ($gargs + @('extract', '--list', $listFile, '-o', $wmOut))
if ($r.Code -ne 0) { Write-Warning "wowsmod extract 退出码 $($r.Code)" }
$pfsOut = Join-Path $base 'pfsunpack'
Invoke-PfsunpackList $pfs $gi $picked $pfsOut
$rows = New-Object System.Collections.Generic.List[object]
$same = 0; $diff = 0; $miss = 0
foreach ($p in $picked) {
    $a = Get-Sha256 (Join-VPath $wmOut $p); $b = Get-Sha256 (Join-VPath $pfsOut $p)
    $st = if (-not $a -or -not $b) { $miss++; 'missing' } elseif ($a -eq $b) { $same++; 'same' } else { $diff++; 'DIFF' }
    $rows.Add([pscustomobject]@{ path = $p; min_block = $min[$p]; status = $st; wowsmod = $a; pfsunpack = $b })
}
$rows | Export-Csv -NoTypeInformation -Encoding UTF8 (Join-Path $base 'files.csv')
$rep = [ordered]@{ entries_with_small_blocks = $min.Count; max_small = $MaxSmall; small_count = $small.Count; random_count = $pickRest.Count
    compared = $picked.Count; identical = $same; different = $diff; missing = $miss; wowsmod_extract_exit = $r.Code
    small_paths = $small }
$rep | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 (Join-Path $base 'report.json')
Write-Host "比较 $($picked.Count) 个: 一致 $same, 不一致 $diff, 缺失 $miss"
if ($diff -gt 0 -or $miss -gt 0) { exit 1 }
