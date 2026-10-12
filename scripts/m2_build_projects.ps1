# M2 验收: 生成两个 mod 项目(只写 out\m2_projects, 不碰游戏目录)
#  1) rt_loose  : DDS 读→写往返(loose 覆盖): _a/_mg 经 PNG 导出→重新编码, _n 去掉 BC7Prep 写成普通 BC7
#  2) deepsea   : SDK 路线, 整船深海蓝(保留细节) + 烟囱发蓝光
# -MfmSet 'emissionAnimationMode=Int:1;animEmissionPower=Float:30'  (多个用 ; 分隔: powershell -File 不能传数组, 逗号会被当成值的一部分)
# -KeepCamoMask   保留 _mg.B 迷彩遮罩(船体显示游戏迷彩配色, 只有遮罩外的部分显示新颜色)
# -ScreenColor    想在港口里看到的颜色(如 #1E3F66): 用 wowsmod port-albedo 换算出贴图该用的 -Color(勃艮第港口标定, 近似)
# -RedControlPng  对照实验: 把船体 skinned 材质(烟囱顶盖等)的漫反射指向一张新的纯色贴图, 证明 .mfm 的属性被游戏读取
param([string]$Color = '#1E3F66', [string]$GlowColor = '#2E9BFF', [double]$Power = 0.25, [string]$DsName = 'deepsea2', [switch]$KeepCamoMask, [string]$MfmSet = '', [string]$RedControlPng = '', [string]$ScreenColor = '', [switch]$Force)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Set-Location (Split-Path $PSScriptRoot -Parent)
$W = '.\target\release\wowsmod.exe'
$root = 'out\m2_projects'
if ($ScreenColor) { $pa = & $W --json port-albedo $ScreenColor | ConvertFrom-Json; $Color = $pa.to_color; Write-Host "港口观感 $ScreenColor → 贴图 --to-color $Color" }
$tex = 'content/gameplay/france/ship/battleship/textures'
$glbDir = 'out\accept_export\bourgogne_lod0'
function Run([string[]]$a) { & $W @a; if ($LASTEXITCODE -ne 0) { throw "wowsmod $($a -join ' ') 失败($LASTEXITCODE)" } }
New-Item -ItemType Directory -Force $root | Out-Null

# ---------- 1) 往返 ----------
$rt = Join-Path $root 'rt_loose'
if ($Force -and (Test-Path $rt)) { if (Test-Path "$rt\.install\manifest.json") { throw "$rt 仍处于安装状态" }; Remove-Item -Recurse -Force $rt }
if (-not (Test-Path $rt)) {
  Run @('mod', 'init', $rt, '--name', 'RoundTrip', '--ship', 'FSB510_Bourgogne', '--mode', 'loose')
  $png = Join-Path $root 'rt_png'; New-Item -ItemType Directory -Force $png | Out-Null
  foreach ($n in 'Hull_a', 'Hull_mg', 'DeckHouse_a', 'DeckHouse_mg') {
    $v = "$tex/FSB025_Bourgogne_1945_$n.dds"
    Run @('tex', 'export', $v, '-o', "$png\$n.png")
    Run @('--json', 'tex', 'import', "$png\$n.png", '--like', $v, '--project', $rt, '--quality', 'high') | Out-Null
  }
  foreach ($n in 'Hull_n', 'DeckHouse_n') {
    Run @('--json', 'tex', 'rewrite', "$tex/FSB025_Bourgogne_1945_$n.dds", '--project', $rt) | Out-Null
  }
}

# ---------- 2) 深海蓝 + 发光 ----------
$ds = Join-Path $root $DsName
if ($Force -and (Test-Path $ds)) { if (Test-Path "$ds\.install\manifest.json") { throw "$ds 仍处于安装状态" }; Remove-Item -Recurse -Force $ds }
if (-not (Test-Path $ds)) {
  Run @('mod', 'init', $ds, '--name', 'DeepSeaBourgogne', '--ship', 'FSB510_Bourgogne', '--mode', 'sdk')
  Run @('mod', 'import-sdk', $ds, '<WORK_DIR>\m0\ingame\mods\exp2_sdk\extracted_WowsModTest', '--sdk-dir', '<WORK_DIR>\m0\ingame\mods\exp2_sdk\res_mods\PnFMods\ModsSDK.zip')
  $prev = Join-Path $root "${DsName}_preview"; New-Item -ItemType Directory -Force $prev | Out-Null
  # 船体/上层建筑 + 法系主副炮/指挥仪/雷达: 只改涂装(排除 0–65° 色相: 木甲板、锈迹、船底红)
  $targets = Get-ChildItem "$ds\assets\res_mods\PnFMods\DeepSeaBourgogne" -Recurse -Filter '*_a.dds' | Where-Object { $_.Name -match '^(FSB025_Bourgogne_1945_(Hull|DeckHouse)_a|F[GDR][A-Z]?\d+.*_a)\.dds$' }
  foreach ($t in $targets) {
    Run @('--json', 'tex', 'recolor', $t.Name, '--project', $ds, '--to-color', $Color, '--exclude-hue', '0:65', '--soft', '0.04', '--preview', "$prev\$($t.BaseName).png", '--quality', 'high') | Out-Null
    Write-Host "换色 $($t.Name)"
    # _mg 的 B 通道 = 迷彩/涂装遮罩(M2 港口实验发现: 船体 B=1 处显示 camouflages.xml 的配色而不是 _a)。
    # 清零后 _a 的颜色才会显示。DeckHouse_mg 由下面的 glow 处理(发光材质里 B 是发光强度)。
    if (-not $KeepCamoMask -and $t.Name -notmatch 'DeckHouse_a') {
      $mg = $t.Name -replace '_a\.dds$', '_mg.dds'
      if (Test-Path (Join-Path $t.DirectoryName $mg)) {
        Run @('--json', 'tex', 'channel', $mg, '--project', $ds, '--channel', 'b', '--set', '0', '--quality', 'high') | Out-Null
        Write-Host "清零迷彩遮罩 $mg"
      }
    }
  }
  # 烟囱发光: MidBack 的 DeckHouse_skinned 部件(rs3)
  Run (@('glow', '--project', $ds, '--mg', 'FSB025_Bourgogne_1945_DeckHouse_mg.dds', '--diffuse', 'FSB025_Bourgogne_1945_DeckHouse_a.dds', '--color', $GlowColor,
        '--glb', "$glbDir\FSB025_Bourgogne_1945_MidBack.glb", '--material', 'DeckHouse_skinned', '--mfm', 'FSB025_Bourgogne_1945_DeckHouse_skinned', '--power', "$Power", '--preview', "$prev\glow") + (($MfmSet -split ';' | Where-Object { $_.Trim() }) | ForEach-Object { '--mfm-set', $_.Trim() }))
  if ($RedControlPng) {
    $hs = Get-ChildItem "$ds\assets\res_mods\PnFMods\DeepSeaBourgogne" -Recurse -Filter 'FSB025_Bourgogne_1945_Hull_skinned.mfm' | Select-Object -First 1
    $dst = Join-Path $hs.DirectoryName 'ctrl_red_a.dds'
    Run @('--json', 'tex', 'import', $RedControlPng, '--like', "$tex/FSB025_Bourgogne_1945_Hull_a.dds", '-o', $dst, '--quality', 'high') | Out-Null
    $t = [IO.File]::ReadAllText($hs.FullName)
    $t2 = [regex]::Replace($t, '(<property>\s*diffuseMap\s*<Texture>\s*)([^<\s]+)(\s*</Texture>)', { param($m) $m.Groups[1].Value + 'PnFMods/DeepSeaBourgogne/FSB510_Bourgogne/ship/ctrl_red_a.dds' + $m.Groups[3].Value })
    if ($t2 -eq $t) { throw '没找到 Hull_skinned.mfm 的 diffuseMap' }
    [IO.File]::WriteAllText($hs.FullName, $t2, (New-Object Text.UTF8Encoding $false))
    Write-Host "对照: $($hs.Name) diffuseMap → ctrl_red_a.dds"
  }
  Run @('mask', '--glb', "$glbDir\FSB025_Bourgogne_1945_MidBack.glb", '--material', 'DeckHouse_skinned', '--like', "$tex/FSB025_Bourgogne_1945_DeckHouse_a.dds", '-o', "$prev\funnel_mask.png", '--overlay', "$prev\funnel_overlay.png")
}
Run @('mod', 'status', $rt)
Run @('mod', 'status', $ds)
