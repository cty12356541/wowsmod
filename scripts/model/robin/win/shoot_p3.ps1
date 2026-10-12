param([string]$Tag='a1',[string]$Proj='<WORK_DIR>\wowsmod\out\e2e\robin\dir_p3')
# 工作台同角出图(对标标杆 6 视 + 夜): focus 时 dist 无效, 近景用 fill(>1 拉近)
$env:WOWSMOD_HOME='<WORK_DIR>\wowsmod\out\m4\home'; $env:WOWSMOD_BOOK_IGNORE='1'
$W='<WORK_DIR>\wowsmod\target\release\wowsmod.exe'; $R='<USERPROFILE>\e2e_port\p3'; $O="$R\shots_$Tag"; $J="$R\j"
New-Item -ItemType Directory -Force $O,$J | Out-Null
$c='&w=1280&h=720&env=water&water=full'
$v=[ordered]@{
 'day_fq'="view=custom&az=140&el=18&dist=22$c&light=day";
 'day_side'="view=custom&az=95&el=10&dist=13$c&light=day";
 'day_fq_high'="view=custom&az=155&el=30&dist=8.5&focus=hull%2FBow$c&light=day";
 'day_top'="view=custom&az=100&el=60&dist=15$c&light=day";
 'day_mid_close'="view=custom&az=60&el=30&focus=hull%2FMidFront&fill=1.9$c&light=day";
 'day_bow_close'="view=custom&az=110&el=36&focus=hull%2FBow$c&light=day";
 'day_stern'="view=custom&az=210&el=16&focus=hull%2FStern&fill=1.1$c&light=day";
 'night_fq'="view=custom&az=140&el=18&dist=22$c&light=night";
 'night_top'="view=custom&az=100&el=55&dist=15$c&light=night";
 'night_side'="view=custom&az=95&el=10&dist=13$c&light=night" }
foreach($k in $v.Keys){ & $W --json-out "$J\r_$k.json" render $Proj --query $v[$k] -o "$O\p3_$k.png" | Out-Null }
tar -czf "$R\shots_$Tag.tgz" -C $O .
(Get-Item "$R\shots_$Tag.tgz").Length
