#!/bin/bash
# 特效预览出图: fx_shots.sh <proj> <outdir> [fx=project|vanilla|JSON]
# 航行(满速 20s 稳态: 航迹+烟囱烟; 艉后高 + 侧面) / 开火(主炮齐射后 0.09s 炮口焰; 0.6s 开火烟), game-day / game-night
export WOWSMOD_GAME=${WOWSMOD_GAME:-./work/m1/fakegame} WOWSMOD_BOOK_IGNORE=1
: "${WOWSMOD_GP:=}" "${WOWSMOD_GEOM_DIR:=}"   # 无完整客户端时(盒子)设: 船 GP JSON / 原版 .geometry 目录
W=${WOWSMOD_BIN:-wowsmod}; P=$1; O=$2; FX=${3:-project}; mkdir -p $O
SAIL=${SAIL:-"view=custom&az=165&el=16&dist=24&fov=35&tgt=0,1,3"}
SIDE=${SIDE:-"view=custom&az=120&el=14&dist=20&fov=35&tgt=0,1,1"}
FIRE=${FIRE:-"view=custom&az=35&el=12&dist=12&fov=35&tgt=0,1.2,-3"}
SMOKE=${SMOKE:-"view=custom&az=70&el=8&dist=11&fov=35&tgt=0,1.4,-2"}
shot(){ $W render $P --look $2 --size 1600x900 --query "env=water&fx=$FX&$3" -o $O/$1.png >/dev/null 2>&1 || echo "FAIL $1"; }
for L in day night; do
  shot sail_$L game-$L "$SAIL&speed=full&warm=20"
  shot sailside_$L game-$L "$SIDE&speed=full&warm=20"
  shot fire_$L game-$L "$FIRE&speed=q2&warm=8&fire=main&fireT=${FIRET:-0.09}"
  shot firesmoke_$L game-$L "$SMOKE&speed=q2&warm=8&fire=main&fireT=0.6"
done
