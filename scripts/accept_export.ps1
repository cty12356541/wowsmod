<#
验收 M1-4: geometry → GLB, 按 assets.bin 的 mapping id 配对 + 健康检查(越界/尖刺) + 包围盒独立校验。
  - 勃艮第 FSB025_Bourgogne_1945 与黎塞留 FSB003_Richelieu_1944 全部船体分段 + 主炮 FGM049;
  - 主模型(LOD0)与全部 LOD(--lod all)各导出一次;
  - 额外: 两船 GameParams 引用到的全部模型目录(炮塔/防空/雷达等)里的 geometry;
  - 对照: naive(数组下标)配对, 证明检查器能抓到错误配对。
结果: out\accept_export\report.json; 主验收项全部通过时退出码 0。
#>
param([string]$Game, [string]$Exe, [string[]]$Ships = @('PFSB510', 'PFSB108'))
. "$PSScriptRoot\_common.ps1"
$wm = Get-WowsmodExe $Exe
$gargs = @(); if ($Game) { $gargs = @('--game', $Game) }
$base = New-CleanDir (Join-Path $Root 'out\accept_export')

function Export-Set([string]$Name, [string]$Glob, [string[]]$Extra) {
    $dir = Join-Path $base $Name
    $r = Invoke-WowsmodJson $wm ($gargs + @('export', $Glob, '-o', $dir) + $Extra)
    if (-not $r.Json -or -not $r.Json.results) { throw "export $Glob 失败: $($r.Raw)" }
    $r.Json.results | Select-Object @{n = 'set'; e = { $Name } }, input, pairing, primitives, triangles, oob_triangles, spike_triangles, long_triangles_info, bbox_checked, bbox_mismatch, winding_normal_agreement, winding_normal_agreement_raw, winding_flipped_primitives, reread_ok, ok, @{n = 'warnings'; e = { $_.warnings -join ' | ' } }
}

$main = @()
$main += Export-Set 'bourgogne_lod0' 'content/gameplay/france/ship/battleship/FSB025_Bourgogne_1945/*.geometry' @()
$main += Export-Set 'richelieu_lod0' 'content/gameplay/france/ship/battleship/FSB003_Richelieu_1944/*.geometry' @()
$main += Export-Set 'fgm049_lod0' 'content/gameplay/france/gun/main/FGM049_380mm_45_Mle_1935/*.geometry' @()
$lods = @()
$lods += Export-Set 'bourgogne_all_lods' 'content/gameplay/france/ship/battleship/FSB025_Bourgogne_1945/*.geometry' @('--lod', 'all')
$lods += Export-Set 'richelieu_all_lods' 'content/gameplay/france/ship/battleship/FSB003_Richelieu_1944/*.geometry' @('--lod', 'all')
$lods += Export-Set 'fgm049_all_lods' 'content/gameplay/france/gun/main/FGM049_380mm_45_Mle_1935/*.geometry' @('--lod', 'all')

# 两船引用的全部模型目录
$extra = @()
foreach ($s in $Ships) {
    $f = Invoke-WowsmodJson $wm ($gargs + @('find', '--ship', $s))
    $hit = $f.Json.ships | Where-Object key -eq $s | Select-Object -First 1
    foreach ($d in $hit.model_dirs) {
        $n = ($d -split '/')[-1]
        $probe = Invoke-WowsmodJson $wm ($gargs + @('find', "$d/*.geometry"))
        if ($probe.Json.count -gt 0) { $extra += Export-Set "ship_${s}_$n" "$d/*.geometry" @() }
    }
}

# 对照: naive
$naive = @()
$naive += Export-Set 'naive_bourgogne' 'content/gameplay/france/ship/battleship/FSB025_Bourgogne_1945/*.geometry' @('--pairing', 'naive')
$naive += Export-Set 'naive_richelieu' 'content/gameplay/france/ship/battleship/FSB003_Richelieu_1944/*.geometry' @('--pairing', 'naive')

function Sum($rows) {
    [ordered]@{
        files = @($rows).Count; ok = @($rows | Where-Object ok).Count
        triangles = ($rows | Measure-Object triangles -Sum).Sum
        oob = ($rows | Measure-Object oob_triangles -Sum).Sum
        spikes = ($rows | Measure-Object spike_triangles -Sum).Sum
        bbox_checked = ($rows | Measure-Object bbox_checked -Sum).Sum
        bbox_mismatch = ($rows | Measure-Object bbox_mismatch -Sum).Sum
        assets_bin_paired = @($rows | Where-Object { $_.pairing -like 'assets.bin*' }).Count
    }
}
$report = [ordered]@{
    main = Sum $main; all_lods = Sum $lods; ship_models = Sum $extra; naive = Sum $naive
    main_rows = $main; lod_rows = $lods; ship_model_rows = $extra; naive_rows = $naive
}
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 (Join-Path $base 'report.json')
$main + $lods + $extra | Format-Table set, @{n = 'file'; e = { ($_.input -split '/')[-1] } }, primitives, triangles, oob_triangles, spike_triangles, bbox_checked, bbox_mismatch, @{n = 'wind'; e = { '{0:P2}' -f $_.winding_normal_agreement } }, ok -AutoSize | Out-String -Width 220 | Write-Host
Write-Host ("naive 对照: " + ($report.naive | ConvertTo-Json -Compress))
$pass = ($report.main.ok -eq $report.main.files) -and ($report.all_lods.ok -eq $report.all_lods.files) -and ($report.main.assets_bin_paired -eq $report.main.files)
if ($pass) { Write-Host "通过: 主验收 $($report.main.files) 个文件 + 全 LOD 0 越界 / 0 尖刺 / 包围盒全部一致" -ForegroundColor Green; exit 0 }
Write-Host '未通过, 见 out\accept_export\report.json' -ForegroundColor Red; exit 1
