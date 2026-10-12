param([string]$Proj='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p3',[string]$Spec='<USERPROFILE>\e2e_port\p3\props_spec.json',[string]$Mod='')
# apply 后自检(r32): 每个外带件必须进入「会被加载的」visual(船体分段或炮塔), 发光材质必须是 emissive fx。
# 失败即非 0 退出, 不要 install。根因: model animate 只把 .ssm 挂进 .model, 网格在独立 .model 里从未被加载 → 羽翼在游戏里消失, 而 apply 报 ok=True。
if(-not $Mod){ $Mod=([regex]::Match((Get-Content (Join-Path $Proj 'mod.toml') -Raw),'(?m)^name\s*=\s*"([^"]+)"')).Groups[1].Value }
$D=Get-Item (Join-Path $Proj "assets\res_mods\PnFMods\$Mod")   # 只查本 mod(ModsSDK.zip 是 SDK 占位)
$cfg=Get-Content $Spec -Raw -Encoding UTF8 | ConvertFrom-Json; $bad=0
$vis=Get-ChildItem $D.FullName -Recurse -Filter *.visual | ? { $_.FullName -notmatch '\\misc\\' }
foreach($p in $cfg.props){
  $hit=$vis | ? { Select-String -Path $_.FullName -Pattern "<vertices>\s*$($p.name)(Shape)?\.vertices" -Quiet }
  if(-not $hit){ "FAIL $($p.name): 不在任何船体/炮塔 visual 里(不会显示)"; $bad++ } else { "ok   $($p.name) in $(($hit | % Name) -join ',')" }
  $m=Get-ChildItem $D.FullName -Recurse -Filter "$($p.name).mfm" | Select-Object -First 1
  if($m){ $fx=[regex]::Match((Get-Content $m.FullName -Raw),'<fx>\s*(\S+)\s*</fx>').Groups[1].Value; if($fx -notmatch 'emissive'){ "FAIL $($p.name): fx=$fx 非 emissive"; $bad++ } } else { "FAIL $($p.name): 缺 mfm"; $bad++ }
}
foreach($k in 'Hull','DeckHouse'){
  Get-ChildItem $D.FullName -Recurse -Filter "*_$k.mfm" | % { $fx=[regex]::Match((Get-Content $_.FullName -Raw),'<fx>\s*(\S+)\s*</fx>').Groups[1].Value; if($fx -notmatch 'emissive'){ "FAIL $($_.Name) fx=$fx"; $bad++ } else { "ok   $($_.Name) emissive" } }
}
$anim=Get-ChildItem $D.FullName -Recurse -Filter *.model | ? { Select-String -Path $_.FullName -Pattern '\.ssm' -Quiet }
if($anim){ "WARN 有 .ssm 动画挂在: $(($anim | % Name) -join ','); 确认网格在加载的 visual 里" }
"RESULT bad=$bad"; exit $bad
