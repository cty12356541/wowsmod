<#
准备集成测试样本到 testdata\(不入库, 版权归游戏方):
  testdata\geometry18\  18 个 .geometry(黎塞留 FSB003/FSB016/FSB017 各 5 个船体文件 + FGM049/FGS024/FGS052)
  testdata\assets.bin, testdata\GameParams_py3.data
  testdata\pkg\        trails.idx + 对应 pkg(小, 含多块 Oodle 条目), 从游戏目录只读复制
然后: $env:WOWSMOD_REQUIRE_SAMPLES=1; cargo test --release
#>
param([string]$Game, [string]$Exe)
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
Write-Host "样本已就绪: $td"
Get-ChildItem -Recurse $td | Where-Object { -not $_.PSIsContainer } | Format-Table FullName, Length -AutoSize | Out-String -Width 200 | Write-Host
