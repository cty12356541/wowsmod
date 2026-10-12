<#
准备集成测试样本到 testdata\(不入库, 版权归游戏方):
  testdata\geometry18\  18 个 .geometry(黎塞留 FSB003/FSB016/FSB017 各 5 个船体文件 + FGM049/FGS024/FGS052)
  testdata\assets.bin, testdata\GameParams_py3.data
  testdata\pkg\        trails.idx + 对应 pkg(小, 含多块 Oodle 条目), 从游戏目录只读复制
  testdata\bc7prep\{raw,ref}\  BC7Prep 贴图: raw = wowsmod extract --raw(pkg 原始字节), ref = pfsunpack 输出
                       (需要 pfsunpack; 找不到时跳过此项并警告)
然后: $env:WOWSMOD_REQUIRE_SAMPLES=1; cargo test --release
#>
param([string]$Game, [string]$Exe, [string]$Pfsunpack)
. "$PSScriptRoot\_common.ps1"
$wm = Get-WowsmodExe $Exe
$gi = Get-GameInfo $wm $Game
$gargs = @(); if ($Game) { $gargs = @('--game', $Game) }
$td = Join-Path $Root 'testdata'
$tmp = New-CleanDir (Join-Path $Root 'out\fetch_samples')
$list = @()
foreach ($s in 'FSB003_Richelieu_1944', 'FSB016_Richelieu_1944_3D_skin_Royal', 'FSB017_Richelieu_1944_3D_skin_Repub') {
    foreach ($seg in '', '_Bow', '_MidFront', '_MidBack', '_Stern') { $list += "content/gameplay/france/ship/battleship/$s/$s$seg.geometry" }
}
$list += 'content/gameplay/france/gun/main/FGM049_380mm_45_Mle_1935/FGM049_380mm_45_Mle_1935.geometry'
$list += 'content/gameplay/france/gun/secondary/FGS024_100mm_45_Mle_1930/FGS024_100mm_45_Mle_1930.geometry'
$list += 'content/gameplay/france/gun/secondary/FGS052_152_55_Mle_1936/FGS052_152_55_Mle_1936.geometry'
$list += 'content/assets.bin', 'content/GameParams_py3.data'
$lf = Join-Path $tmp 'list.txt'; $list | Set-Content -Encoding UTF8 $lf
$r = Invoke-WowsmodJson $wm ($gargs + @('extract', '--list', $lf, '-o', $tmp))
if ($r.Code -ne 0) { throw "解包失败: $($r.Raw)" }
New-Item -ItemType Directory -Force (Join-Path $td 'geometry18'), (Join-Path $td 'pkg') | Out-Null
Get-ChildItem -Recurse -Filter *.geometry $tmp | Copy-Item -Destination (Join-Path $td 'geometry18') -Force
Copy-Item (Join-Path $tmp 'content\assets.bin'), (Join-Path $tmp 'content\GameParams_py3.data') $td -Force
Copy-Item (Join-Path $gi.idx_dir 'trails.idx') (Join-Path $td 'pkg') -Force
Get-ChildItem $gi.res_packages -Filter 'trails_*.pkg' | Copy-Item -Destination (Join-Path $td 'pkg') -Force
# ---- BC7Prep 样本: 两条整船里的 BC7Prep 贴图 + 若干不同 flags/模式组合的贴图(含模式 7 的 LNT047)
$pfs = $null; try { $pfs = Get-PfsunpackExe $Pfsunpack } catch { Write-Warning "跳过 BC7Prep 样本: $_" }
if ($pfs) {
    $probe = Join-Path $tmp 'bc7probe'
    $kw = @('FD033_Rangefinder_n.dd0', 'FGM049_380mm_45_Mle_1935_n.dd0', 'FGM549_430_50_Mle_1940_Grimlock_TF_n.dd1', 'IAD500_Savoia_Marchetti_SM93_Lazio_Ceremonial_n.dd1', 'LMY421_n.dd0', 'LNT047_Jin_Chan_rnb.dd1',
        # flags 第 16 位(颜色去相关)样本
        'particles/animated/Arpeggio_balls_2x1.dd0', 'particles/animated/Bubbles_6x6.dd0', 'particles/animated/Firework_fat_8x8.dd0', 'particles/animated/Smoke_curved_7x7_LM.dd1',
        'LNT000_dock_SPB_02_rnb.dd1', 'LNT000_Dock_ChinaArpeggio_08_rnb.dd0', 'ZSD101_Huang_Long_3Kings_Hull_art.dd0',
        # 去相关 + 模式 2
        'particles/animated/Pointer_APR24_1x30.dd0', 'particles/animated/Pointer_APR24_1x30.dd1', 'dyndecals/exterior/PCED005_d.dd0', 'maps/decals/55_Seychelles/55_Seychelles_LA_road_alpha_a.dd2')
    $r = Invoke-WowsmodJson $wm ($gargs + @('extract', '--ship', 'PFSB510') + $kw + @('-o', $probe))
    $r2 = Invoke-WowsmodJson $wm ($gargs + @('extract', '--ship', 'PFSB108', '-o', $probe))
    $prep = @(@($r.Json.bc7prep_files) + @($r2.Json.bc7prep_files) | Sort-Object -Unique)
    Write-Host "BC7Prep 样本: $($prep.Count) 个"
    $bl = Join-Path $tmp 'bc7prep_list.txt'; $prep | Set-Content -Encoding UTF8 $bl
    $rawDir = Join-Path $tmp 'bc7raw'; $refDir = Join-Path $tmp 'bc7ref'
    $r3 = Invoke-WowsmodJson $wm ($gargs + @('extract', '--raw', '--list', $bl, '-o', $rawDir))
    if ($r3.Code -ne 0) { throw "extract --raw 失败: $($r3.Raw)" }
    Invoke-PfsunpackList $pfs $gi $prep $refDir
    $dst = Join-Path $td 'bc7prep'
    if (Test-Path $dst) { Remove-Item -Recurse -Force $dst }
    New-Item -ItemType Directory -Force (Join-Path $dst 'raw'), (Join-Path $dst 'ref') | Out-Null
    foreach ($p in $prep) {
        $n = ($p -replace '^content/', '') -replace '/', '__'
        Copy-Item -LiteralPath (Join-VPath $rawDir $p) (Join-Path $dst "raw\$n")
        Copy-Item -LiteralPath (Join-VPath $refDir $p) (Join-Path $dst "ref\$n")
    }
}
Write-Host "样本已就绪: $td"
Get-ChildItem -Recurse $td | Where-Object { -not $_.PSIsContainer } | Format-Table FullName, Length -AutoSize | Out-String -Width 200 | Write-Host
