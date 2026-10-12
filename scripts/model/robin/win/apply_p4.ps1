param([string]$Spec='<USERPROFILE>\e2e_port\p4\props_spec_T3.json',[double]$GlowPower=0.7,[string]$R='<USERPROFILE>\e2e_port\p4',[int]$Prune=1,[int]$FlatN=1,[int]$Despeckle=1)
# P4/T3: 发光强度 0.7(夜景读主题色、白天可见); 原壳降噪 model prune; 末尾自动 verify_apply, 失败即报错退出(不要 install)
# PowerShell 变量不分大小写: 勿用 $spec 覆盖 [string]$Spec 参数
# 确定性重建: dir_p2(贴图层定稿) → dir_p3 镜像, 再按 spec 逐件 import 外带(避免反复 import 的同名渲染集残留)
$env:WOWSMOD_HOME='<WORK_DIR>\wowsmod\out\m4\home'; $env:WOWSMOD_BOOK_IGNORE='1'
$W='<WORK_DIR>\wowsmod\target\release\wowsmod.exe'; $J="$R\j"; New-Item -ItemType Directory -Force $J | Out-Null
$src='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p2'; $dst='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p3'
robocopy $src $dst /MIR /NFL /NDL /NJH /NJS /NP | Out-Null
# P3 贴图层增量(发光线/锚色/天青发光色): 覆盖 dir_p2 的 _a, 再按遮罩写 _mg B + 切 emissive
$T="$R\tex"
if(Test-Path "$T\hull_a_p2_half.png"){
  $imp=[ordered]@{ 'hull_a_p2_half.png'='FSB025_Bourgogne_1945_Hull_a.dds'; 'dh_a_p2_half.png'='FSB025_Bourgogne_1945_DeckHouse_a.dds'; 'FGM049_a_p2_half.png'='FGM049_380mm_45_Mle_1935_a.dds'; 'FGS036_a_p2_half.png'='FGS036_127mm_54_5_1948_a.dds'; 'FGS053_a_p2_half.png'='FGS053_152_55_Mle_1950_a.dds' }
  # P5: 原壳降噪「融」并入流水线: 船舷遮罩内 _a 去小暗斑(舷窗/流锈), 遮罩由 paint 产出(hull_side_mask.png), 不写死 UV 行带
  $src_of=@{}; foreach($k in $imp.Keys){ $src_of[$k]="$T\$k" }
  if($Despeckle -and (Test-Path "$T\hull_side_mask.png")){
    $py='<PYTHON_DIR>\Programs\Python\Python312\python.exe'
    & $py '<WORK_DIR>\wowsmod\scripts\paint\despeckle.py' "$T\hull_a_p2_half.png" "$J\hull_a_ds.png" --mask "$T\hull_side_mask.png" --thr 7 --report "$J\despeckle.json"
    if($LASTEXITCODE -ne 0){ Write-Error 'despeckle 失败'; exit 1 }; $src_of['hull_a_p2_half.png']="$J\hull_a_ds.png"
  }
  foreach($k in $imp.Keys){ & $W --json-out "$J\t_$k.json" tex import $src_of[$k] --like $imp[$k] --project $dst | Out-Null; "tex $k " + ((Get-Content "$J\t_$k.json" -Raw -Encoding UTF8) -match '"ok": true') }
  $gl=[ordered]@{ 'Hull'=@('hull_glow.png','FSB025_Bourgogne_1945_Hull_mg.dds',@('FSB025_Bourgogne_1945_Hull.mfm','FSB025_Bourgogne_1945_Hull_skinned.mfm'));
                  'DeckHouse'=@('dh_glow.png','FSB025_Bourgogne_1945_DeckHouse_mg.dds',@('FSB025_Bourgogne_1945_DeckHouse.mfm','FSB025_Bourgogne_1945_DeckHouse_skinned.mfm')) }
  foreach($k in $gl.Keys){ $g=$gl[$k]; $a=@('--json-out',"$J\glow_$k.json",'glow','--project',$dst,'--mg',$g[1],'--mask',"$T\$($g[0])",'--power',"$GlowPower")
    foreach($m in $g[2]){ $a+=@('--mfm',$m) }
    & $W @a | Out-Null; $res=Get-Content "$J\glow_$k.json" -Raw -Encoding UTF8 | ConvertFrom-Json; "glow $k ok=$($res.ok) err=$($res.error)" }
}
$cfg=Get-Content $Spec -Raw -Encoding UTF8 | ConvertFrom-Json
foreach($p in $cfg.props){
  $verb='import'; if($p.anim){ $verb='animate' }   # A7: 悬浮翼 → model animate --preset float(缓慢上下浮动)
  # P5 教训: 图集必须与 glb 同目录同批次(旧版取 $R\prop_atlas.png, 换了 glb 却没换图集 → 进游戏仍是旧图集)
  $a=@('--json-out',"$J\imp_$($p.name).json",'model',$verb,'--project',$dst,'--mesh',"$R\props\$($p.name).glb",'--like',$p.like,'--name',$p.name,
       '--texture',"$R\props\prop_atlas.png",'--glow',"$R\props\prop_glow.png",'--power',"$($cfg.power)",'--tex-size','1024','--gloss',"$($cfg.gloss)")
  if($p.section){ $a+=@('--section',$p.section) }; if($p.part){ $a+=@('--part',$p.part) }; if($p.hp){ $a+=@('--hp',$p.hp) }
  if($p.node){ $a+=@('--node',$p.node) }; if($p.at){ $a+=@('--at',$p.at) }; if($p.rot){ $a+=@('--rot',$p.rot) }; if($p.scale){ $a+=@('--scale',"$($p.scale)") }
  if($p.anim){ $a+=@('--preset',$p.anim.preset,'--float-amp',"$($p.anim.amp)",'--period',"$($p.anim.period)") }
  & $W @a | Out-Null
  $res=Get-Content "$J\imp_$($p.name).json" -Raw -Encoding UTF8 | ConvertFrom-Json
  "$($p.name) ok=$($res.ok) err=$($res.error) warn=$(($res.warnings | % { $_ }) -join ';')"
}
if($Prune){   # 原壳降噪(shape.refine.clean): 舷侧小件 + 主甲板小件(y<0.75), 全 LOD 同条件, 只退化三角形
  foreach($pr in @(@('side','0.08',''),@('up','0.05','-2,0,-10,2,0.75,10'))){
    $a=@('--json-out',"$J\prune_$($pr[0]).json",'model','prune','--project',$dst,'--renderset','^(Bow|MidFront|MidBack|Stern)$','--facing',$pr[0],'--max-size',$pr[1])
    if($pr[2]){ $a+=@("--box=$($pr[2])") }
    & $W @a | Out-Null; $res=Get-Content "$J\prune_$($pr[0]).json" -Raw -Encoding UTF8 | ConvertFrom-Json; "prune $($pr[0]) ok=$($res.ok) err=$($res.error)"
  }
}
if($FlatN){   # 原壳降噪「融」: 舷窗/铆钉/小舱口凹凸在 Hull_n 法线里(几何删不掉) → 船体法线 R/G 置 128(平); B/A 不动
  Get-ChildItem $dst -Recurse -Filter '*_Hull_n.dds' | % {
    $mk=@(); if(Test-Path "$T\hull_side_mask.png"){ $mk=@('--mask',"$T\hull_side_mask.png",'--feather','2') }   # P5: 只压舷侧, 保留甲板木纹
    foreach($c in 'r','g'){ & $W --json-out "$J\flatn_$c.json" tex channel $_.Name --project $dst --channel $c --set 128 @mk | Out-Null
      $res=Get-Content "$J\flatn_$c.json" -Raw -Encoding UTF8 | ConvertFrom-Json; "flatN $($_.Name) $c ok=$($res.ok) err=$($res.error)" } }
}
# r32/T3: apply 后自检, 失败即报错
& powershell -NoProfile -ExecutionPolicy Bypass -File '<WORK_DIR>\wowsmod\scripts\model\robin\verify_apply.ps1' -Proj $dst -Spec $Spec
if($LASTEXITCODE -ne 0){ Write-Error "verify_apply 失败 $LASTEXITCODE 项: 不要 install"; exit 1 }
"APPLY_OK"
