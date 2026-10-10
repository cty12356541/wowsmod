# M2 验收: 生成两个 mod 项目(只写 out\m2_projects, 不碰游戏目录)
#  1) rt_loose  : DDS 读→写往返(loose 覆盖): _a/_mg 经 PNG 导出→重新编码, _n 去掉 BC7Prep 写成普通 BC7
#  2) deepsea   : SDK 路线, 整船深海蓝(保留细节) + 烟囱发蓝光
param([string]$Color = '#1E3F66', [string]$GlowColor = '#2E9BFF', [double]$Power = 4.0, [string]$DsName = 'deepsea2', [switch]$KeepCamoMask, [string[]]$MfmSet = @(), [switch]$Force)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Set-Location (Split-Path $PSScriptRoot -Parent)
$W = '.\target\release\wowsmod.exe'
$root = 'out\m2_projects'
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
  Run @('mod', 'import-sdk', $ds, 'D:\WoWSModding\m0\ingame\mods\exp2_sdk\extracted_WowsModTest', '--sdk-dir', 'D:\WoWSModding\m0\ingame\mods\exp2_sdk\res_mods\PnFMods\ModsSDK.zip')
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
        '--glb', "$glbDir\FSB025_Bourgogne_1945_MidBack.glb", '--material', 'DeckHouse_skinned', '--mfm', 'FSB025_Bourgogne_1945_DeckHouse_skinned', '--power', "$Power", '--preview', "$prev\glow") + ($MfmSet | ForEach-Object { '--mfm-set', $_ }))
  Run @('mask', '--glb', "$glbDir\FSB025_Bourgogne_1945_MidBack.glb", '--material', 'DeckHouse_skinned', '--like', "$tex/FSB025_Bourgogne_1945_DeckHouse_a.dds", '-o', "$prev\funnel_mask.png", '--overlay', "$prev\funnel_overlay.png")
}
Run @('mod', 'status', $rt)
Run @('mod', 'status', $ds)
