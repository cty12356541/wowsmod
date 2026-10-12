param([string]$Tag='v2')
$env:WOWSMOD_HOME='<WORK_DIR>\wowsmod\out\m4\home'; $env:WOWSMOD_BOOK_IGNORE='1'
$W='<WORK_DIR>\wowsmod\target\release\wowsmod.exe'; $P='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p2'; $O="<USERPROFILE>\e2e_port\p2\shots_$Tag"; $J='<USERPROFILE>\e2e_port\p2\j'
New-Item -ItemType Directory -Force $O | Out-Null
$v=[ordered]@{
 'day_fq'='view=custom&az=140&el=18&dist=22&w=1280&h=720&env=water&water=full&light=day';
 'day_side'='view=custom&az=95&el=10&dist=13&w=1280&h=720&env=water&water=full&light=day';
 'day_fq_high'='view=custom&az=155&el=30&dist=8.5&focus=hull%2FBow&w=1280&h=720&env=water&water=full&light=day';
 'day_top'='view=custom&az=100&el=60&dist=15&w=1280&h=720&env=water&water=full&light=day';
 'day_mid_close'='view=custom&az=60&el=30&dist=3.2&focus=hull%2FMidFront&w=1280&h=720&env=water&water=full&light=day';
 'day_bow_close'='view=custom&az=110&el=36&dist=5&focus=hull%2FBow&w=1280&h=720&env=water&water=full&light=day';
 'day_stern'='view=custom&az=200&el=12&dist=10&focus=hull%2FStern&w=1280&h=720&env=studio&light=day';
 'night_fq'='view=custom&az=140&el=18&dist=22&w=1280&h=720&env=water&water=full&light=night';
 'night_top'='view=custom&az=100&el=55&dist=15&w=1280&h=720&env=water&water=full&light=night' }
foreach($k in $v.Keys){ & $W --json-out "$J\r_$k.json" render $P --query $v[$k] -o "$O\p2_$k.png" | Out-Null }
tar -czf "<USERPROFILE>\e2e_port\p2\shots_$Tag.tgz" -C $O .
(Get-Item "<USERPROFILE>\e2e_port\p2\shots_$Tag.tgz").Length



