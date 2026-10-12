#!/bin/bash
# 整船四视角 game-day: ship_views.sh <proj> <outdir>
export WOWSMOD_GAME=${WOWSMOD_GAME:-./work/m1/fakegame} WOWSMOD_BOOK_IGNORE=1
W=./target/release/wowsmod; P=$1; O=$2; mkdir -p $O
shot(){ $W render $P --look game-day --size 1280x720 --query "$2" -o $O/$1.png >/dev/null 2>&1 || echo "FAIL $1"; }
shot high_oblique "view=custom&az=145&el=32&dist=12.5&fov=35&tgt=0,0.8,0"
shot side         "view=custom&az=90&el=5&dist=14&fov=35&tgt=0,0.8,0"
shot bow_high     "view=custom&az=200&el=40&dist=10&fov=35&tgt=0,0.8,-2"
shot top          "view=custom&az=180&el=85&dist=15&fov=35&tgt=0,0.5,0"
