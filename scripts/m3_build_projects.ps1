# M3 验收: 生成模型写回的各个 mod 项目(只写 out\m3_projects, 不碰游戏目录)
#   base       对照(只 import-sdk)
#   rebuild    等价重建: 全部 geometry 解码→编码器从零写出(保留原切线字节)
#   rebuildt   等价重建(切线/副法线也由编码器重算)
#   scalehp    挂点矩阵缩放: HP_FGM_3(后主炮)×1.5
#   scalepart  部件缩放: FGM049(三座主炮同型)×1.5(挂点缩放不被引擎采用时的备选)
#   move       挂点平移: 副炮 HP_FGS_8 / HP_FGS_9 上移 0.25
#   replace    部件替换: FGS036(127mm 双联副炮, 8 座) → FGS053(152mm 三联)
#   remove     删除渲染集: MidBack 烟囱三件(Chimney)
#   newmesh    新网格: 烟囱顶加帽+桅杆(GLB, 整船坐标)
#   blender    Blender 往返: 导出 MidBack → Blender 拉高烟囱 → 导回
param([string[]]$Only = @(), [switch]$Force, [string]$Blender = '<WORK_DIR>\tools\blender-5.2.2-windows-x64\blender.exe')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Set-Location (Split-Path $PSScriptRoot -Parent)
$W = (Resolve-Path '.\target\release\wowsmod.exe').Path
$root = 'out\m3_projects'
$logs = 'out\m3_projects\_logs'
New-Item -ItemType Directory -Force $root, $logs | Out-Null
function Run([string]$log, [string[]]$a) {
  $o = & $W @a 2>&1 | Out-String
  Add-Content -Encoding UTF8 (Join-Path $logs "$log.log") ("> wowsmod " + ($a -join ' ') + "`r`n" + $o)
  if ($LASTEXITCODE -ne 0) { throw "wowsmod $($a -join ' ') 失败($LASTEXITCODE): $o" }
  $o
}
function NewProj([string]$name) {
  $p = Join-Path $root $name
  if (Test-Path $p) {
    if (-not $Force) { return $null }
    if (Test-Path "$p\.install\manifest.json") { throw "$p 仍处于安装状态" }
    Remove-Item -Recurse -Force $p
  }
  Run $name @('mod', 'init', $p, '--name', "M3$name", '--ship', 'FSB510_Bourgogne', '--mode', 'sdk') | Out-Null
  Run $name @('mod', 'import-sdk', $p, '<WORK_DIR>\m0\ingame\mods\exp2_sdk\extracted_WowsModTest', '--sdk-dir', '<WORK_DIR>\m0\ingame\mods\exp2_sdk\res_mods\PnFMods\ModsSDK.zip') | Out-Null
  $p
}
function Want([string]$n) { ($Only.Count -eq 0) -or ($Only -contains $n) }

if (Want 'base') { $p = NewProj 'base'; if ($p) { Write-Host "base: $p" } }
if (Want 'rebuild') { $p = NewProj 'rebuild'; if ($p) { Run 'rebuild' @('--json', 'model', 'rebuild', '--project', $p) | Out-Null; Write-Host "rebuild: $p" } }
if (Want 'rebuildt') { $p = NewProj 'rebuildt'; if ($p) { Run 'rebuildt' @('--json', 'model', 'rebuild', '--project', $p, '--recompute-tangents') | Out-Null; Write-Host "rebuildt: $p" } }
if (Want 'scalehp') { $p = NewProj 'scalehp'; if ($p) { Run 'scalehp' @('--json', 'model', 'scale', '--project', $p, '--hp', 'HP_FGM_3', '1.5') | Out-Null; Write-Host "scalehp: $p" } }
if (Want 'scalepart') { $p = NewProj 'scalepart'; if ($p) { Run 'scalepart' @('--json', 'model', 'scale', '--project', $p, '--part', 'FGM049', '1.5') | Out-Null; Write-Host "scalepart: $p" } }
if (Want 'move') {
  $p = NewProj 'move'
  if ($p) { foreach ($h in 'HP_FGS_8', 'HP_FGS_9') { Run 'move' @('--json', 'model', 'move', '--project', $p, '--hp', $h, '--by', '0,0.25,0') | Out-Null }; Write-Host "move: $p" }
}
if (Want 'replace') { $p = NewProj 'replace'; if ($p) { Run 'replace' @('--json', 'model', 'replace', '--project', $p, '--part', 'FGS036', '--with', 'FGS053') | Out-Null; Write-Host "replace: $p" } }
if (Want 'remove') { $p = NewProj 'remove'; if ($p) { Run 'remove' @('--json', 'model', 'remove', '--project', $p, '--section', 'MidBack', '--renderset', 'Chimney') | Out-Null; Write-Host "remove: $p" } }
if (Want 'newmesh') {
  $p = NewProj 'newmesh'
  if ($p) { Run 'newmesh' @('--json', 'model', 'add', '--project', $p, '--section', 'MidBack', '--glb', 'scripts\m3\funnel_cap.glb', '--like', '^MidBack_DeckHouse$', '--name', 'FunnelCap') | Out-Null; Write-Host "newmesh: $p" }
}
if (Want 'blender') {
  $p = NewProj 'blender'
  if ($p) {
    $bd = Join-Path $root '_blender'; New-Item -ItemType Directory -Force $bd | Out-Null
    Run 'blender' @('--json', 'model', 'export-blender', '--project', $p, '--section', 'MidBack', '-o', "$bd\midback_in.glb") | Out-Null
    $eap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    $bo = & $Blender --background --factory-startup --python 'scripts\blender\m3_roundtrip.py' -- "$bd\midback_in.glb" "$bd\midback_out.glb" 1.35 2>&1 | Out-String
    $ErrorActionPreference = $eap
    Set-Content -Encoding UTF8 (Join-Path $logs 'blender_run.log') $bo
    if (-not (Test-Path "$bd\midback_out.glb")) { throw "Blender 没有导出 GLB: $bo" }
    Run 'blender' @('--json', 'model', 'import-blender', '--project', $p, "$bd\midback_out.glb") | Out-Null
    Write-Host "blender: $p"
  }
}
Get-ChildItem $root -Directory | Where-Object Name -notlike '_*' | ForEach-Object { Write-Host ("{0,-10} edits={1}" -f $_.Name, (Get-ChildItem "$($_.FullName)\edits" -Filter *.json).Count) }
