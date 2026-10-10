# 进游戏验收用的小工具(从 M0 的 m0\ingame\tools 改写; 坐标一律为物理像素, 进程先 SetProcessDPIAware)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;using System.Runtime.InteropServices;
public class GU{
[DllImport("user32.dll")]public static extern bool SetProcessDPIAware();
[DllImport("user32.dll")]public static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")]public static extern void mouse_event(uint f,uint dx,uint dy,int d,UIntPtr e);
[DllImport("user32.dll")]public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e);
[DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")]public static extern bool ShowWindow(IntPtr h,int c);
[DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out RECT r);
[DllImport("user32.dll")]public static extern int GetSystemMetrics(int i);
[DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
public struct RECT{public int L,T,R,B;}}
"@
[GU]::SetProcessDPIAware() | Out-Null
function Get-Screen { [pscustomobject]@{ W = [GU]::GetSystemMetrics(0); H = [GU]::GetSystemMetrics(1) } }
function Get-WinRect([string]$proc) {
  $p = Get-Process $proc -EA 0 | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1
  if (-not $p) { return $null }
  $r = New-Object GU+RECT; [GU]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
  [pscustomobject]@{ Proc = $p.ProcessName; Pid = $p.Id; L = $r.L; T = $r.T; R = $r.R; B = $r.B; Handle = $p.MainWindowHandle }
}
function Get-ForegroundProc { $h = [GU]::GetForegroundWindow(); $id = 0; [GU]::GetWindowThreadProcessId($h, [ref]$id) | Out-Null; (Get-Process -Id $id -EA 0).ProcessName }
function Focus-Proc([string]$proc) {
  $w = Get-WinRect $proc; if (-not $w) { return $false }
  [GU]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [GU]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  [GU]::ShowWindow($w.Handle, 9) | Out-Null; [GU]::SetForegroundWindow($w.Handle) | Out-Null; Start-Sleep -Milliseconds 400
  return ((Get-ForegroundProc) -eq $proc)
}
# 点击: 只有前台是指定进程时才点(防止点到别的窗口)
function Click-At([int]$x, [int]$y, [string]$expectProc = 'Korabli64', [string]$btn = 'left') {
  $fg = Get-ForegroundProc
  if ($expectProc -and $fg -ne $expectProc) { throw "前台窗口是 $fg 不是 $expectProc, 取消点击" }
  [GU]::SetCursorPos($x, $y) | Out-Null; Start-Sleep -Milliseconds 150
  if ($btn -eq 'left') { [GU]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80; [GU]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero) }
  elseif ($btn -eq 'right') { [GU]::mouse_event(8, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80; [GU]::mouse_event(16, 0, 0, 0, [UIntPtr]::Zero) }
}
function Drag-Right([int]$x, [int]$y, [int]$dx, [int]$dy, [int]$steps = 20) {
  if ((Get-ForegroundProc) -ne 'Korabli64') { throw '前台不是游戏, 取消拖动' }
  [GU]::SetCursorPos($x, $y) | Out-Null; Start-Sleep -Milliseconds 100
  [GU]::mouse_event(8, 0, 0, 0, [UIntPtr]::Zero)
  for ($i = 1; $i -le $steps; $i++) { [GU]::SetCursorPos($x + [int]($dx * $i / $steps), $y + [int]($dy * $i / $steps)) | Out-Null; Start-Sleep -Milliseconds 25 }
  [GU]::mouse_event(16, 0, 0, 0, [UIntPtr]::Zero)
}
function Wheel([int]$x, [int]$y, [int]$delta) {
  if ((Get-ForegroundProc) -ne 'Korabli64') { throw '前台不是游戏, 取消滚轮' }
  [GU]::SetCursorPos($x, $y) | Out-Null; Start-Sleep -Milliseconds 100; [GU]::mouse_event(0x800, 0, 0, $delta, [UIntPtr]::Zero)
}
function Shot([string]$out, [int]$scale = 2) {
  $s = Get-Screen
  $bmp = New-Object System.Drawing.Bitmap $s.W, $s.H
  $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen(0, 0, 0, 0, $bmp.Size); $g.Dispose()
  New-Item -ItemType Directory -Force (Split-Path $out) | Out-Null
  $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $small = $out -replace '\.png$', '_small.png'
  $sm = New-Object System.Drawing.Bitmap $bmp, ([int]($s.W / $scale)), ([int]($s.H / $scale)); $sm.Save($small, [System.Drawing.Imaging.ImageFormat]::Png); $sm.Dispose(); $bmp.Dispose()
  $small
}
