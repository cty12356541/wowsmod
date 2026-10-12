# M4 进港验证: .wsmod 包 → 装到模拟的旧版本 res_mods → migrate 到当前版本 → 进港看效果 → 退出 → 卸载 → 核对复原。
# 先 . .\scripts\ingame\common.ps1; . .\scripts\ingame\m3_game.ps1; . .\scripts\ingame\m4_game.ps1
$M4Old = '<WORK_DIR>\wowsmod\out\m4\fake_bin\8863954\res_mods'
function Run-M4Session {
  Assert-ResModsClean
  $w = '.\target\release\wowsmod.exe'
  $env:WOWSMOD_HOME = '<WORK_DIR>\wowsmod\out\m4\home'
  if (Test-Path (Split-Path $M4Old)) { Remove-Item -Recurse -Force (Split-Path $M4Old) }
  New-Item -ItemType Directory -Force $M4Old | Out-Null
  Set-Content -Encoding ascii "$M4Old\readme.txt" 'fake old build res_mods'
  $pk = & $w --json pack out\m2_projects\deepsea6 -o out\m4\dist | ConvertFrom-Json
  if (-not $pk.ok) { throw 'pack 失败' }
  $i = & $w --json install $pk.package --res-mods $M4Old | ConvertFrom-Json
  if (-not $i.ok) { throw '包安装到旧目录失败' }
  $m = & $w --json migrate --from $M4Old | ConvertFrom-Json
  if (-not $m.ok) { throw "migrate 失败: $($m | ConvertTo-Json -Depth 6)" }
  $v = & $w --json installed --verify | ConvertFrom-Json
  if (-not $v.intact) { throw 'migrate 后校验不通过' }
  $d = '<WORK_DIR>\wowsmod\out\m4_ingame\migrated'
  $o = @(Start-Port $d 100)
  if ((Get-ForegroundProc) -ne 'Korabli64') { throw '进港后前台不是游戏' }
  $o += Take-Views $d; $o += Take-Extra $d
  $log = Session-Log "$d\python_session.log"
  $st = '<USERPROFILE>\m4x'; New-Item -ItemType Directory -Force $st | Out-Null; Remove-Item "$st\*" -Force -EA 0
  $o | Where-Object { $_ -like '*_small.png' } | ForEach-Object { Copy-Item $_ "$st\m4_$(Split-Path $_ -Leaf)" }
  $q = Quit-Ask $d; Copy-Item $q "$st\q.png" -Force
  Push-Location $st; tar -czf "$st\..\m4x.tgz" *.png; Pop-Location
  [pscustomobject]@{ package = $pk.package; pkg_sha256 = $pk.sha256; migrated_to = $m.to; files = $m.mods[0].result.files; old_uninstalled = ($m.old_uninstalled | ConvertTo-Json -Compress -Depth 4); verify_intact = $v.intact; mods = (($v.mods | % { $_.name }) -join ','); log_lines = $log.lines; registered = [bool]$log.registered; replaced = $log.replaced; unexpected = $log.unexpected; crash = @(Crash-Check) }
}
function End-M4Session {
  Quit-Confirm; Start-Sleep 4
  $u = & .\target\release\wowsmod.exe --json uninstall DeepSeaBourgogne | ConvertFrom-Json
  Assert-ResModsClean
  $left = @(Get-ChildItem -Recurse -Force $M4Old | % FullName)
  [pscustomobject]@{ uninstall_ok = $u.ok; deleted = $u.deleted; pristine = $u.res_mods_pristine; old_dir_left = ($left -join ',') }
}
