# M3 港口验收辅助(先 . .\scripts\ingame\common.ps1)。坐标为物理像素(2560x1440, DPI-aware)。
# 只在港口: 不进战斗、不花钱、不领奖励; 每次点击前核对窗口与坐标。
$Script:LgcRect = '257,110,2049,1230'
function Assert-ResModsClean {
  $f = Get-ChildItem 'D:\Korabli\bin\8867689\res_mods' -Recurse -Force -File
  if ($f.Count -ne 1 -or $f[0].Name -ne 'readme.txt') { throw "res_mods 不干净: $($f.FullName -join ', ')" }
  if ((Get-FileHash $f[0].FullName -Algorithm SHA256).Hash -ne '0157BF7594E9ADFF105A9E3214FD70B8106A22D9F4B6964F383CB4CA95CA2F3F') { throw 'readme.txt 哈希不符' }
  'res_mods 干净(只有 readme.txt, 哈希一致)'
}
function Start-Port([string]$shotDir, [int]$waitSec = 100) {
  if (Get-Process Korabli64 -EA 0) { throw '游戏已在运行' }
  # 启动器常驻托盘时没有主窗口; 再次 Start-Process 会把已有实例的窗口调出来
  if (-not (Get-WinRect 'lgc')) { Start-Process 'D:\Lesta\GameCenter\lgc.exe'; Start-Sleep 12 }
  for ($i = 0; $i -lt 30; $i++) { $w = Get-WinRect 'lgc'; if ($w) { break }; Start-Sleep 2 }
  if (-not $w) { throw '没有找到启动器窗口' }
  $r = "$($w.L),$($w.T),$($w.R),$($w.B)"
  if ($r -ne $Script:LgcRect) { throw "启动器窗口矩形 $r 与预期 $Script:LgcRect 不符, 不点击" }
  if (-not (Focus-Proc 'lgc')) { throw '无法把启动器置前' }
  Click-At 570 1145 'lgc'
  Start-Sleep $waitSec
  Shot (Join-Path $shotDir 'port_loaded.png')
}
function RelDrag([int]$n, [int]$dx = 15, [int]$dy = 0) {
  if ((Get-ForegroundProc) -ne 'Korabli64') { throw '前台不是游戏, 取消拖动' }
  [GU]::SetCursorPos(1280, 720) | Out-Null; Start-Sleep -Milliseconds 120
  [GU]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  for ($i = 0; $i -lt $n; $i++) { [GU]::mouse_event(1, [uint32]$dx, [uint32]$dy, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 20 }
  [GU]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 600
}
function Zoom([int]$clicks) {
  # clicks>0: 拉远(滚轮向后), <0: 拉近
  $d = if ($clicks -gt 0) { -120 } else { 120 }
  for ($i = 0; $i -lt [math]::Abs($clicks); $i++) { Wheel 1280 720 $d; Start-Sleep -Milliseconds 250 }
  Start-Sleep -Milliseconds 800
}
function Crop([string]$src, [string]$dst, [int]$x, [int]$y, [int]$w, [int]$h) {
  $b = [System.Drawing.Bitmap]::FromFile($src); $c = $b.Clone((New-Object System.Drawing.Rectangle $x, $y, $w, $h), $b.PixelFormat)
  $c.Save($dst, [System.Drawing.Imaging.ImageFormat]::Png); $c.Dispose(); $b.Dispose(); $dst
}
function Quit-Ask([string]$shotDir) {
  if (-not (Focus-Proc 'Korabli64')) { throw '无法把游戏置前' }
  [GU]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [GU]::keybd_event(0x73, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [GU]::keybd_event(0x73, 0, 2, [UIntPtr]::Zero); [GU]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero); Start-Sleep 2
  Shot (Join-Path $shotDir 'quit_dialog.png')
}
function Quit-Confirm { Click-At 1185 712 'Korabli64'; Start-Sleep 8; if (Get-Process Korabli64 -EA 0) { 'still running' } else { 'game closed' } }
function Save-PythonLog([string]$dst) { Copy-Item 'D:\Korabli\profile\python.log' $dst -Force; $t = Get-Content $dst -Raw; [pscustomobject]@{ lines = ($t -split "`n").Count; errors = ([regex]::Matches($t, '(?im)^.*(Traceback|ERROR|EXCEPTION|PnFMods|contentSdk).*$') | Select-Object -First 40 | ForEach-Object Value) } }
function Take-Views([string]$d) {
  # 固定机位: v1 船尾斜视 / v2 侧视 / v3 拉远 / v4 拉近(舯部), 各次会话一致便于对比
  $o = @()
  RelDrag 60; Start-Sleep 1; $o += Shot "$d\v1_stern.png"
  for ($i = 0; $i -lt 4; $i++) { Wheel 1280 720 -120; Start-Sleep -Milliseconds 250 }; Start-Sleep 1
  RelDrag 20; Start-Sleep 1; $o += Shot "$d\v2_side.png"
  for ($i = 0; $i -lt 12; $i++) { Wheel 1280 720 -120; Start-Sleep -Milliseconds 250 }; Start-Sleep 2; $o += Shot "$d\v3_far.png"
  for ($i = 0; $i -lt 22; $i++) { Wheel 1280 720 120; Start-Sleep -Milliseconds 250 }; Start-Sleep 2; $o += Shot "$d\v4_near.png"
  $o
}
function Session-Log([string]$dst) {
  # 只取本次启动(最后一个 "Invalid paths xml" 起)的日志, 去掉已知与模组无关的报错
  $l = Get-Content 'D:\Korabli\profile\python.log' -Encoding UTF8
  $s = ($l | Select-String 'Invalid paths xml' | Select-Object -Last 1).LineNumber
  $sess = $l[($s - 1)..($l.Count - 1)]; $sess | Set-Content $dst -Encoding UTF8
  $known = 'Invalid paths xml|Unknown IME|ModsSDK.zip|RewardSourceSystem|XmppChat'
  [pscustomobject]@{ lines = $sess.Count
    registered = ($sess | Select-String 'registerShipMod').Line
    replaced = ($sess | Select-String '\] Replace ').Count
    unexpected = @($sess | Select-String 'ERROR|Traceback|EXCEPTION|CRITICAL' | Where-Object { $_.Line -notmatch $known } | ForEach-Object { $_.Line }) }
}
function Crash-Check { Get-ChildItem 'D:\Korabli' -Recurse -Depth 2 -Include '*.dmp','crash*' -EA 0 | Where-Object LastWriteTime -gt (Get-Date).AddHours(-1) | ForEach-Object FullName }
function Take-Extra([string]$d) {
  # 接在 Take-Views 之后: 拉远 8 -> 6x 旋转 25 -> 拉近 5 -> 8x 旋转 12 (x14 船尾斜后方) -> 拉近 4 (x15 后主炮特写) -> 拉远 14 (x16 远景/LOD)
  $o = @()
  for ($i = 0; $i -lt 8; $i++) { Wheel 1280 720 -120; Start-Sleep -Milliseconds 250 }; Start-Sleep 1
  for ($k = 1; $k -le 6; $k++) { RelDrag 25; Start-Sleep 1 }
  for ($i = 0; $i -lt 5; $i++) { Wheel 1280 720 120; Start-Sleep -Milliseconds 250 }; Start-Sleep 1
  for ($k = 7; $k -le 14; $k++) { RelDrag 12; Start-Sleep 1 }
  $o += Shot "$d\x14.png"
  for ($i = 0; $i -lt 4; $i++) { Wheel 1280 720 120; Start-Sleep -Milliseconds 250 }; Start-Sleep 1.5; $o += Shot "$d\x15.png"
  for ($i = 0; $i -lt 14; $i++) { Wheel 1280 720 -120; Start-Sleep -Milliseconds 250 }; Start-Sleep 2; $o += Shot "$d\x16.png"
  $o
}
function Run-Session([string]$n) {
  # 一次会话的前半段: 核对 res_mods -> 安装 -> 进港 -> 固定机位截图 -> 日志; 退出对话框由调用方截图核对后再点
  Assert-ResModsClean
  $r = & .\target\release\wowsmod.exe --json install --dev "out\m3_projects\$n" | ConvertFrom-Json
  if (-not $r.ok) { throw "安装失败 $n" }
  $d = "<WORK_DIR>\wowsmod\out\m3_ingame\$n"
  $o = @(Start-Port $d 100)
  if ((Get-ForegroundProc) -ne 'Korabli64') { throw '进港后前台不是游戏' }
  $o += Take-Views $d; $o += Take-Extra $d
  $log = Session-Log "$d\python_session.log"
  $st = '<USERPROFILE>\m3x'; Remove-Item "$st\*" -Force -EA 0
  $o | Where-Object { $_ -like '*_small.png' } | ForEach-Object { Copy-Item $_ "$st\$n`_$(Split-Path $_ -Leaf)" }
  $q = Quit-Ask $d; Copy-Item $q "$st\q.png" -Force
  Push-Location $st; tar -czf "$st\..\m3x_$n.tgz" *.png; Pop-Location
  [pscustomobject]@{ project = $n; files = $r.files; log_lines = $log.lines; registered = [bool]$log.registered; replaced = $log.replaced; unexpected = $log.unexpected; crash = @(Crash-Check) }
}
function End-Session([string]$n) { Quit-Confirm; Start-Sleep 4; & .\target\release\wowsmod.exe uninstall "out\m3_projects\$n" | Select-Object -Last 1; Assert-ResModsClean }
