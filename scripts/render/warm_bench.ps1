# 常驻出图实例计时(Windows). 用法: powershell -File warm_bench.ps1 <wowsmod.exe> <项目(会被改动, 用副本!)> <要改色的贴图 .dds>
param([string]$exe, [string]$p, [string]$tex)
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$r = "$p\renders"; New-Item -ItemType Directory -Force $r | Out-Null
function J($args2) { $o = & $exe --json @args2 | Out-String; try { $o | ConvertFrom-Json } catch { $null } }
function T([scriptblock]$b) { $sw = [Diagnostics.Stopwatch]::StartNew(); $x = & $b; [pscustomobject]@{ s = $sw.Elapsed.TotalSeconds; v = $x } }
$q = "view=fq&compare=split&base=base&w=1600&h=900"
& $exe render stop $p | Out-Null
$c = T { J @("render", $p, "--query", $q, "--cold", "-o", "$r\b_cold.png") }; "cold          {0,6:N2}s  warm={1}" -f $c.s, $c.v.warm
$w = T { J @("render", "--warm", $p) }; "warm-up       {0,6:N2}s  {1}" -f $w.s, ($w.v.warmup | ConvertTo-Json -Compress)
$s = T { J @("render", $p, "--query", $q, "-o", "$r\b_w1.png") }; "warm single   {0,6:N2}s  warm={1} server={2:N3}s" -f $s.s, $s.v.warm, $s.v.timing.seconds
$views = "fq", "side", "top", "bq", "fq", "side", "top", "bq", "fq", "side"; $ts = @()
$all = T { for ($i = 0; $i -lt 10; $i++) { $x = T { J @("render", $p, "--query", "view=$($views[$i])&compare=split&base=base&w=1600&h=900", "-o", "$r\b_s$i.png") }; $script:ts += $x.s } }
"10 in a row   {0,6:N2}s total, avg {1:N2}s, max {2:N2}s" -f $all.s, ($ts | Measure -Average).Average, ($ts | Measure -Maximum).Maximum
$hue = Get-Random -Minimum 30 -Maximum 300
& $exe tex recolor $tex --project $p --hue-shift $hue -o $tex | Out-Null
$i1 = T { J @("render", $p, "--query", $q, "-o", "$r\b_inc.png") }; "after change  {0,6:N2}s  reload={1}" -f $i1.s, ($i1.v.timing.reload | ConvertTo-Json -Compress -Depth 5)
$i2 = T { J @("render", $p, "--query", $q, "-o", "$r\b_inc2.png") }; "next render   {0,6:N2}s" -f $i2.s
