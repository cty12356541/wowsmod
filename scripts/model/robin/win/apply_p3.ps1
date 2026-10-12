param([string]$Spec='<USERPROFILE>\e2e_port\p3\props_spec.json',[double]$GlowPower=0.35)
# PowerShell 变量不分大小写: 勿用 $spec 覆盖 [string]$Spec 参数
# 确定性重建: dir_p2(贴图层定稿) → dir_p3 镜像, 再按 spec 逐件 import 外带(避免反复 import 的同名渲染集残留)
$env:WOWSMOD_HOME='<WORK_DIR>\wowsmod\out\m4\home'; $env:WOWSMOD_BOOK_IGNORE='1'
$W='<WORK_DIR>\wowsmod\target\release\wowsmod.exe'; $R='<USERPROFILE>\e2e_port\p3'; $J="$R\j"
$src='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p2'; $dst='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p3'
robocopy $src $dst /MIR /NFL /NDL /NJH /NJS /NP | Out-Null
# P3 贴图层增量(发光线/锚色/天青发光色): 覆盖 dir_p2 的 _a, 再按遮罩写 _mg B + 切 emissive
$T="$R\tex"
if(Test-Path "$T\hull_a_p2_half.png"){
  $imp=[ordered]@{ 'hull_a_p2_half.png'='FSB025_Bourgogne_1945_Hull_a.dds'; 'dh_a_p2_half.png'='FSB025_Bourgogne_1945_DeckHouse_a.dds'; 'FGM049_a_p2_half.png'='FGM049_380mm_45_Mle_1935_a.dds'; 'FGS036_a_p2_half.png'='FGS036_127mm_54_5_1948_a.dds'; 'FGS053_a_p2_half.png'='FGS053_152_55_Mle_1950_a.dds' }
  foreach($k in $imp.Keys){ & $W --json-out "$J\t_$k.json" tex import "$T\$k" --like $imp[$k] --project $dst | Out-Null; "tex $k " + ((Get-Content "$J\t_$k.json" -Raw -Encoding UTF8) -match '"ok": true') }
  $gl=[ordered]@{ 'Hull'=@('hull_glow.png','FSB025_Bourgogne_1945_Hull_mg.dds',@('FSB025_Bourgogne_1945_Hull.mfm','FSB025_Bourgogne_1945_Hull_skinned.mfm'));
                  'DeckHouse'=@('dh_glow.png','FSB025_Bourgogne_1945_DeckHouse_mg.dds',@('FSB025_Bourgogne_1945_DeckHouse.mfm','FSB025_Bourgogne_1945_DeckHouse_skinned.mfm')) }
  foreach($k in $gl.Keys){ $g=$gl[$k]; $a=@('--json-out',"$J\glow_$k.json",'glow','--project',$dst,'--mg',$g[1],'--mask',"$T\$($g[0])",'--power',"$GlowPower")
    foreach($m in $g[2]){ $a+=@('--mfm',$m) }
    & $W @a | Out-Null; $res=Get-Content "$J\glow_$k.json" -Raw -Encoding UTF8 | ConvertFrom-Json; "glow $k ok=$($res.ok) err=$($res.error)" }
}
$cfg=Get-Content $Spec -Raw -Encoding UTF8 | ConvertFrom-Json
foreach($p in $cfg.props){
  $verb='import'; if($p.anim){ $verb='animate' }   # A7: 悬浮翼 → model animate --preset float(缓慢上下浮动)
  $a=@('--json-out',"$J\imp_$($p.name).json",'model',$verb,'--project',$dst,'--mesh',"$R\props\$($p.name).glb",'--like',$p.like,'--name',$p.name,
       '--texture',"$R\prop_atlas.png",'--glow',"$R\prop_glow.png",'--power',"$($cfg.power)",'--tex-size','1024','--gloss',"$($cfg.gloss)")
  if($p.section){ $a+=@('--section',$p.section) }; if($p.part){ $a+=@('--part',$p.part) }; if($p.hp){ $a+=@('--hp',$p.hp) }
  if($p.node){ $a+=@('--node',$p.node) }; if($p.at){ $a+=@('--at',$p.at) }; if($p.rot){ $a+=@('--rot',$p.rot) }; if($p.scale){ $a+=@('--scale',"$($p.scale)") }
  if($p.anim){ $a+=@('--preset',$p.anim.preset,'--float-amp',"$($p.anim.amp)",'--period',"$($p.anim.period)") }
  & $W @a | Out-Null
  $res=Get-Content "$J\imp_$($p.name).json" -Raw -Encoding UTF8 | ConvertFrom-Json
  "$($p.name) ok=$($res.ok) err=$($res.error) warn=$(($res.warnings | % { $_ }) -join ';')"
}
