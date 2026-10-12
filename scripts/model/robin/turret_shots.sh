#!/bin/bash
# 炮塔近景(game-day): turret_shots.sh <proj> <outdir>
# 每类: 俯视 / 侧视 / 斜视。转塔/仰角姿态: box 装配几何无 yaw/pitch 骨骼, render turret= 不生效 → 用 turret_pose_render.py
export WOWSMOD_GAME=${WOWSMOD_GAME:-./work/m1/fakegame} WOWSMOD_BOOK_IGNORE=1
# 转塔模拟(turret=/aimKm=)需要 GameParams: 预热前设 WOWSMOD_GP / WOWSMOD_GEOM_DIR
export WOWSMOD_GP=${WOWSMOD_GP:-./work/e2e/calib/gp_bour.json} WOWSMOD_GEOM_DIR=${WOWSMOD_GEOM_DIR:-./work/m3/game/content/gameplay/france}
W=./target/release/wowsmod; P=$1; O=$2; mkdir -p $O
shot(){ $W render $P --look game-day --size 1000x700 --query "$2" -o $O/$1.png >/dev/null 2>&1 || echo "FAIL $1"; }
# FGM_1 主炮(前1) game z 4.07, y 0.6
for T in "fgm1:0,0.7,-4.1:3.6" "fgs053:0,1.1,-1.2:2.6" "fgs036:0.95,0.6,-0.5:2.2" "fgm3:0,0.7,4.8:3.6"; do
  IFS=: read n tg d <<< "$T"
  shot ${n}_top    "view=custom&az=180&el=80&dist=$d&fov=35&tgt=$tg"
  shot ${n}_side   "view=custom&az=90&el=6&dist=$d&fov=35&tgt=$tg"
  qa=150; [ $n = fgs036 ] && qa=60   # fgs036: 150° 斜视被右翼挡住
  shot ${n}_q      "view=custom&az=$qa&el=30&dist=$d&fov=35&tgt=$tg"
done
