#!/bin/bash
# 炮塔近景(game-day): turret_shots.sh <proj> <outdir>
# 每类: 俯视 / 侧视 / 转塔 90° 斜视 / 转塔 90° + 远距仰角(aimKm 28)
export WOWSMOD_GAME=${WOWSMOD_GAME:-/workspace/m1/fakegame} WOWSMOD_BOOK_IGNORE=1
# 转塔模拟(turret=/aimKm=)需要 GameParams: 预热前设 WOWSMOD_GP / WOWSMOD_GEOM_DIR
export WOWSMOD_GP=${WOWSMOD_GP:-/workspace/e2e/calib/gp_bour.json} WOWSMOD_GEOM_DIR=${WOWSMOD_GEOM_DIR:-/workspace/m3/game/content/gameplay/france}
W=/workspace/wm/src/target/release/wowsmod; P=$1; O=$2; mkdir -p $O
shot(){ $W render $P --look game-day --size 1000x700 --query "$2" -o $O/$1.png >/dev/null 2>&1 || echo "FAIL $1"; }
# FGM_1 主炮(前1) game z 4.07, y 0.6
for T in "fgm1:0,0.7,-4.1:3.6" "fgs053:0,1.1,-1.2:2.6" "fgs036:0.95,0.6,-0.5:2.2" "fgm3:0,0.7,4.8:3.6"; do
  IFS=: read n tg d <<< "$T"
  shot ${n}_top    "view=custom&az=180&el=80&dist=$d&fov=35&tgt=$tg"
  shot ${n}_side   "view=custom&az=90&el=6&dist=$d&fov=35&tgt=$tg"
  shot ${n}_q      "view=custom&az=150&el=30&dist=$d&fov=35&tgt=$tg"
  shot ${n}_t90    "view=custom&az=150&el=30&dist=$d&fov=35&tgt=$tg&turret=90&aimKm=8"
  shot ${n}_t90el  "view=custom&az=90&el=6&dist=$d&fov=35&tgt=$tg&turret=90&aimKm=28"
done
