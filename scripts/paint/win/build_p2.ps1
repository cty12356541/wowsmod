param([string]$Tgz)
$env:WOWSMOD_HOME='<WORK_DIR>\wowsmod\out\m4\home'; $env:WOWSMOD_BOOK_IGNORE='1'
$W='<WORK_DIR>\wowsmod\target\release\wowsmod.exe'; $P='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p2'; $S='<USERPROFILE>\e2e_port\p2'; $J="$S\j"
New-Item -ItemType Directory -Force "$S\png",$J | Out-Null
Remove-Item "$S\png\*" -Force -ErrorAction SilentlyContinue
tar -xzf $Tgz -C "$S\png"
$imp=[ordered]@{ 'hull_a_p2_half.png'='FSB025_Bourgogne_1945_Hull_a.dds'; 'dh_a_p2_half.png'='FSB025_Bourgogne_1945_DeckHouse_a.dds'; 'FGM049_a_p2_half.png'='FGM049_380mm_45_Mle_1935_a.dds'; 'FGS036_a_p2_half.png'='FGS036_127mm_54_5_1948_a.dds'; 'FGS053_a_p2_half.png'='FGS053_152_55_Mle_1950_a.dds' }
foreach($k in $imp.Keys){ & $W --json-out "$J\imp_$k.json" tex import "$S\png\$k" --like $imp[$k] --project $P | Out-Null; "$k " + ((Get-Content "$J\imp_$k.json" -Raw -Encoding UTF8) -match '"ok": true') }
$parts=[ordered]@{ 'FGA014_20mm_Oerlikon_MK4'='#775EA4'; 'FGA056_57mm_60_2_25'='#775EA4' }
foreach($k in $parts.Keys){ & $W --json-out "$J\rc_$k.json" tex recolor "$($k)_a.dds" --project $P --to-color $parts[$k] --soft 0.04 | Out-Null; "$k " + ((Get-Content "$J\rc_$k.json" -Raw -Encoding UTF8) -match '"ok": true') }
# 炮管: 原 _mg G(光泽)=245 在晴天下把深蓝反成白紫 → 降光泽
& $W --json-out "$J\mg_C035.json" tex channel 'C035_Gun_Barrel_mg.dds' --project $P --channel g --set 120 | Out-Null; 'C035 mg ' + ((Get-Content "$J\mg_C035.json" -Raw -Encoding UTF8) -match '"ok": true')
# 格栅(C011/C005/C004)盖在艏艉主甲板上, 渲染时成大块浅色三角 → 统一甲板靛色; C004 是 SDK 占位, 用 --from-game 取原图
foreach($k in 'C011_Grid_9_alpha_a.dds','C005_Grid_2_alpha_a.dds'){ & $W --json-out "$J\g_$k.json" tex recolor $k --project $P --to-color '#2E2160' --soft 0.04 | Out-Null }
& $W --json-out "$J\g4.json" tex recolor C004_Grid_1_alpha_a.dds --project $P --from-game --game D:\Korabli --to-color '#2E2160' --soft 0.04 | Out-Null
