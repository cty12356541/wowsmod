# 训练房特效实测(炮口焰 / 开火烟 / 航迹 / 烟囱烟)。先 . .\scripts\ingame\common.ps1 ; . .\fx_training.ps1
# 坐标为物理像素(2560x1440, DPI-aware)。菜单进训练房的点击位置未标定 → Enter-TrainingRoom 只做「人工/代理点菜单」的提示与校验, 战斗内操作全自动。
# 规则: 只用训练房(用户已授权); 不花钱不领奖励; 结束关游戏; 不卸载 res_mods 里的 Robin_Bourgogne; 只增删 Robin_FX 的文件。
$Script:Game = 'D:\Korabli'
$Script:Bin = (Get-ChildItem "$Script:Game\bin" -Directory | Sort-Object { [int]$_.Name } | Select-Object -Last 1).FullName
$Script:ResMods = Join-Path $Script:Bin 'res_mods'
$Script:Out = '<WORK_DIR>\research\fx\training'

function Fx-Files { Get-ChildItem (Join-Path $Script:ResMods 'particles'), (Join-Path $Script:ResMods 'trails') -Recurse -File -EA 0 }
function Assert-RobinShipKept { if (-not (Test-Path (Join-Path $Script:ResMods 'PnFMods\Robin_Bourgogne'))) { Write-Warning 'res_mods\PnFMods\Robin_Bourgogne 不在(按规则不应被本脚本动过)' } else { 'Robin_Bourgogne 仍在' } }
function Assert-FxClean { $f = Fx-Files; if ($f) { throw "res_mods 里还有特效文件: $($f.FullName -join ', ')" }; 'res_mods\particles 与 \trails 已清空' }

# 按键(扫描码无关的虚拟键)
function Key([byte]$vk, [int]$holdMs = 60) { if ((Get-ForegroundProc) -ne 'Korabli64') { throw '前台不是游戏, 取消按键' }; [GU]::keybd_event($vk, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds $holdMs; [GU]::keybd_event($vk, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 120 }
function LClick { if ((Get-ForegroundProc) -ne 'Korabli64') { throw '前台不是游戏' }; [GU]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 70; [GU]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero) }
# 连拍: n 张, 间隔 ms(全屏 CopyFromScreen 约 60–90 ms/张)
function Burst([string]$dir, [string]$tag, [int]$n = 8, [int]$gapMs = 0) { New-Item -ItemType Directory -Force $dir | Out-Null; $t0 = Get-Date; for ($i = 0; $i -lt $n; $i++) { $ms = [int]((Get-Date) - $t0).TotalMilliseconds; Shot (Join-Path $dir ("{0}_{1:D2}_{2:D4}ms.png" -f $tag, $i, $ms)) | Out-Null; if ($gapMs) { Start-Sleep -Milliseconds $gapMs } } }

function Enter-TrainingRoom {
  # 港口: 顶部「战斗」旁的模式下拉 → 训练房(Тренировочная комната) → 创建房间 → 选布艮第(带 Robin 涂装/外带) → 不加机器人(或加 1 艘静止靶舰便于瞄准) → 开始。
  # 由人或 Windows 代理用视觉点击完成, 进入战斗读条后按 Enter 继续。
  Read-Host '进入训练房战斗并看到己舰后按 Enter'
  if (-not (Focus-Proc 'Korabli64')) { throw '无法把游戏置前' }
  Start-Sleep 3
}
function Sail-Wake([string]$dir) {
  # 满速(W 四档) → 40 s 加速到稳态 → 第三人称拉远到能看到整条航迹 → 连拍; 再左满舵 15 s 拍转向航迹
  1..4 | ForEach-Object { Key 0x57 }            # W
  Start-Sleep 40
  Zoom 6; RelDrag 40 0 4                        # 拉远、稍俯视
  Burst $dir 'wake_straight' 4 500
  RelDrag 120 15 0; Burst $dir 'wake_side' 3 400   # 转到侧面: 烟囱烟 + 舷侧白沫
  Key 0x41 2500                                  # A 左舵
  Start-Sleep 15; Burst $dir 'wake_turn' 3 500
  Key 0x44 1200                                  # D 回舵
}
function Fire-Muzzle([string]$dir) {
  # 主炮: 镜头对准船舷外海面(相机朝向 ≈ 炮塔会转到的方位), 等炮塔转到位 → 左键齐射 → 立刻连拍 10 张(覆盖 0–0.8 s: 炮口焰 + 开火烟)
  Zoom -3; RelDrag 60 15 0; Start-Sleep 25        # 炮塔转向(布艮第约 20 s 转 180°)
  LClick; Burst $dir 'fire_main' 10 0
  Start-Sleep 30                                  # 装填 28 s
  Key 0x10 200                                    # Shift 望远镜(近景炮口)
  LClick; Burst $dir 'fire_main_binoc' 8 0
  Key 0x10 200
}
function Exit-Battle([string]$dir) {
  Key 0x1B; Start-Sleep 2; Shot (Join-Path $dir 'esc_menu.png') | Out-Null
  Read-Host '确认「离开战斗」并回到港口后按 Enter(菜单坐标未标定, 请人工/代理点击)'
}
# 一轮: 'vanilla'(不装 Robin_FX)或 'robin'(先 wowsmod install Robin_FX)
function Run-FxSession([string]$tag) {
  $d = Join-Path $Script:Out $tag
  Start-Port $d 100
  Enter-TrainingRoom
  Sail-Wake $d
  Fire-Muzzle $d
  Exit-Battle $d
  Save-PythonLog (Join-Path $d 'python.log') | Format-List
  Quit-Ask $d; Quit-Confirm
  Crash-Check
}
