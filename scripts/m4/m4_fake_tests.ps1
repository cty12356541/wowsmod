# M4 假 res_mods 实测: install/uninstall 精确复原、共存/冲突、改动保护、pack/包安装、migrate。
# 只写 <WORK_DIR>\wowsmod\out\m4\fake 下, 不碰真实游戏目录。用法: .\scripts\m4\m4_fake_tests.ps1
$ErrorActionPreference = 'Continue'
Set-Location <WORK_DIR>\wowsmod
$w = (Resolve-Path .\target\release\wowsmod.exe).Path
$root = '<WORK_DIR>\wowsmod\out\m4\fake'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
New-Item -ItemType Directory -Force $root | Out-Null
$log = Join-Path $root 'log.txt'
$script:fails = 0
function L($s) { $s | Tee-Object -FilePath $log -Append }
function Snap($d) {
  if (-not (Test-Path $d)) { return '' }
  (Get-ChildItem -Recurse -Force -File $d | Sort-Object FullName | % { $_.FullName.Substring($d.Length) + ' ' + (Get-FileHash -Algorithm SHA256 $_.FullName).Hash }) + `
  (Get-ChildItem -Recurse -Force -Directory $d | Sort-Object FullName | % { $_.FullName.Substring($d.Length) + '/' }) -join "`n"
}
function Check($name, $cond) { if ($cond) { L "  [PASS] $name" } else { L "  [FAIL] $name"; $script:fails++ } }
function W { $o = & $w @args 2>&1 | Out-String; $script:code = $LASTEXITCODE; Add-Content $log ("> wowsmod " + ($args -join ' ') + "`n" + $o + "exit=$($script:code)"); return $o }
function NewRes($n) {
  $d = Join-Path $root "$n\res_mods"; New-Item -ItemType Directory -Force $d | Out-Null
  Set-Content -Encoding ascii (Join-Path $d 'readme.txt') 'original readme'
  # 用户自己的文件(与 mod 无关): 应原样保留
  New-Item -ItemType Directory -Force (Join-Path $d 'gui\user') | Out-Null
  Set-Content -Encoding ascii (Join-Path $d 'gui\user\mine.txt') 'user file'
  return $d
}
$ds = 'out\m2_projects\deepsea6'; $rm = 'out\m3_projects\remove'; $lo = 'out\m2_projects\rt_loose'

L "== T1 install/uninstall 精确复原 =="
$r1 = NewRes 'r1'; $s0 = Snap $r1
W install $ds --res-mods $r1 --json | Out-Null; Check 'install deepsea6 exit 0' ($code -eq 0)
$o = W installed --verify --res-mods $r1 --json; Check 'installed --verify 全部一致' ($code -eq 0 -and $o -match 'deepsea|DeepSea')
W uninstall $ds --res-mods $r1 --json | Out-Null; Check 'uninstall exit 0' ($code -eq 0)
Check '卸载后与安装前逐文件(含目录)完全一致' ((Snap $r1) -eq $s0)

L "== T2 共存与冲突 =="
W install $ds --res-mods $r1 --json | Out-Null; Check 'install deepsea6' ($code -eq 0)
$o = W conflicts $rm --res-mods $r1; Check 'conflicts remove(同船 SDK) 报冲突 exit 2' ($code -eq 2)
$o = W install $rm --res-mods $r1 --json; Check '同船第二个 SDK mod 被拒' ($code -ne 0)
Check '被拒后 deepsea6 仍完好' ((W installed --verify --res-mods $r1 --json) -and $code -eq 0)
$o = W conflicts $lo --res-mods $r1; L $o; Check 'loose 贴图 mod 与 deepsea6 可共存' ($code -eq 0)
W install $lo --res-mods $r1 --json | Out-Null; Check 'install rt_loose' ($code -eq 0)
W installed --verify --res-mods $r1 --json | Out-Null; Check '两个 mod 都校验通过' ($code -eq 0)
W uninstall $ds --res-mods $r1 --json | Out-Null; Check '先卸 deepsea6' ($code -eq 0)
W installed --verify --res-mods $r1 --json | Out-Null; Check 'rt_loose 仍完好' ($code -eq 0)
W uninstall $lo --res-mods $r1 --json | Out-Null; Check '再卸 rt_loose' ($code -eq 0)
Check '两个都卸后完全复原' ((Snap $r1) -eq $s0)

L "== T3 同路径已有用户文件: 备份与复原 =="
$r3 = NewRes 'r3'
W install $lo --res-mods $r3 --json | Out-Null
$reg = Get-Content -Raw -Encoding utf8 (Join-Path $r3 '.wowsmod\registry.json') | ConvertFrom-Json
$rels = @($reg.paths.PSObject.Properties | % { $_.Value } | ? { $_.origin -ne 'generated' } | % { $_.rel })
L "  rt_loose 文件数 $($rels.Count), 取 $($rels[0])"
W uninstall $lo --res-mods $r3 --json | Out-Null
$s3 = Snap $r3
if ($rels.Count -gt 0) {
  $p = Join-Path $r3 ($rels[0] -replace '/', '\'); New-Item -ItemType Directory -Force (Split-Path $p) | Out-Null
  Set-Content -Encoding ascii $p 'user version of this file'; $s3 = Snap $r3
  $o = W install $lo --res-mods $r3 --json; Check '目标位置已有用户文件仍可安装(备份)' ($code -eq 0)
  W uninstall $lo --res-mods $r3 --json | Out-Null
  Check '卸载后用户原文件复原、其余一致' ((Snap $r3) -eq $s3)
} else { L "  [SKIP] 未能取得文件列表: $o" }

L "== T4 安装后被改动的文件: 默认保留, --force 删除 =="
$r4 = NewRes 'r4'; $s4 = Snap $r4
W install $lo --res-mods $r4 --json | Out-Null
$f = Get-ChildItem -Recurse -File $r4 | ? { $_.FullName -notmatch '\\\.wowsmod\\|readme.txt|mine.txt' } | Select-Object -First 1
Add-Content $f.FullName 'tamper'
$o = W installed --verify --res-mods $r4 --json; Check 'verify 发现改动(非 0)' ($code -ne 0)
$o = W uninstall $lo --res-mods $r4 --json; L $o; Check '改动文件被保留' (Test-Path $f.FullName)
W install $lo --res-mods $r4 --json | Out-Null; L "  (保留文件后重装 exit=$code)"
W uninstall $lo --res-mods $r4 --force --json | Out-Null
if (Test-Path $f.FullName) { Remove-Item $f.FullName }
# 清理空目录后比较
Get-ChildItem -Recurse -Directory $r4 | Sort-Object { $_.FullName.Length } -Descending | ? { -not (Get-ChildItem -Force $_.FullName) } | Remove-Item
Check '--force 后可回到干净状态' ((Snap $r4) -eq $s4)

L "== T5 pack 与包安装 =="
$pk = Join-Path $root 'pkg'
$o = W pack $ds -o $pk --json; L $o; Check 'pack 成功' ($code -eq 0)
$wz = Get-ChildItem $pk -Filter *.wsmod | Select-Object -First 1
Check '生成 .wsmod' ($null -ne $wz)
if ($wz) {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $z = [IO.Compression.ZipFile]::OpenRead($wz.FullName); $names = $z.Entries | % FullName; $z.Dispose()
  L ("  包内文件数 " + $names.Count + ", 大小 " + $wz.Length)
  Check '包内无 Oodle/pfsunpack/可执行文件' (-not ($names | ? { $_ -match '(?i)oo2core|oodle|pfsunpack|mkut|\.(exe|dll|pyd|so|dylib|bat|cmd|ps1|msi)$' }))
  Check '包内含 wowsmod-package.json 与 README.txt' (($names -contains 'wowsmod-package.json') -and ($names -contains 'README.txt'))
  $env:WOWSMOD_HOME = Join-Path $root 'home'
  $r5 = NewRes 'r5'; $s5 = Snap $r5
  W install $wz.FullName --res-mods $r5 --json | Out-Null; Check '从包安装' ($code -eq 0)
  $o = W installed --verify --res-mods $r5 --json; Check '包安装后校验通过' ($code -eq 0 -and $o -match 'DeepSeaBourgogne')
  # 篡改包 → 拒绝
  $bad = Join-Path $root 'bad.wsmod'; $b = [IO.File]::ReadAllBytes($wz.FullName); $b[[int]($b.Length/2)] = $b[[int]($b.Length/2)] -bxor 0xFF; [IO.File]::WriteAllBytes($bad, $b)
  $r5b = NewRes 'r5b'; W install $bad --res-mods $r5b --json | Out-Null; Check '损坏的包被拒' ($code -ne 0)
  $nm = (Get-Content $ds\mod.toml -Raw) -match 'name = "(\w+)"' | Out-Null; $nm = $matches[1]
  W uninstall $nm --res-mods $r5 --json | Out-Null; Check '按 mod 名卸载' ($code -eq 0)
  Check '包卸载后完全复原' ((Snap $r5) -eq $s5)
  Remove-Item Env:\WOWSMOD_HOME
}

L "== T6 migrate(模拟游戏更新换 bin 目录) =="
$old = NewRes 'bin_old'; $new = NewRes 'bin_new'; $so = Snap $old; $sn = Snap $new
W install $ds --res-mods $old --json | Out-Null; W install $lo --res-mods $old --json | Out-Null
$o = W migrate --from $old --to $new --dry-run --json; Check 'migrate --dry-run' ($code -eq 0)
Check 'dry-run 不改新目录' ((Snap $new) -eq $sn)
$o = W migrate --from $old --to $new --json; L $o; Check 'migrate 成功' ($code -eq 0)
W installed --verify --res-mods $new --json | Out-Null; Check '新目录两个 mod 校验通过' ($code -eq 0)
Check '旧目录已复原' ((Snap $old) -eq $so)
W uninstall $ds --res-mods $new --json | Out-Null; W uninstall $lo --res-mods $new --json | Out-Null
Check '新目录卸载后复原' ((Snap $new) -eq $sn)

L "== 结果: 失败 $script:fails 项 =="
