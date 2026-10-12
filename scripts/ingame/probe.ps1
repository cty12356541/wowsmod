. "$PSScriptRoot\common.ps1"
$s = Get-Screen; "screen(physical) $($s.W)x$($s.H)"
Add-Type -AssemblyName System.Windows.Forms
"WinForms PrimaryScreen: $([System.Windows.Forms.Screen]::PrimaryScreen.Bounds)"
$dpi = (Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -EA 0).AppliedDPI; "AppliedDPI=$dpi"
foreach ($p in 'lgc', 'Korabli64') { $w = Get-WinRect $p; if ($w) { "$p pid=$($w.Pid) rect=$($w.L),$($w.T),$($w.R),$($w.B)" } else { "${p}: 无窗口" } }
"foreground: $(Get-ForegroundProc)"
