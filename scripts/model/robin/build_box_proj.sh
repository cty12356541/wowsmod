#!/bin/bash
# 盒子上重建知更鸟 A7(只读 p3/p2 素材, 写到 ./work/e2e/calib/proj_A7)
set -e
export WOWSMOD_GAME=./work/m1/fakegame WOWSMOD_BOOK_IGNORE=1
W=./target/release/wowsmod; D=${PROJ:-./work/e2e/robin/p3/turret/proj_T1}; T=${TEX:-./work/e2e/robin/p2/out9}; P=${PROPS:-./work/e2e/robin/p3/turret/props_T1}; SPEC=${SPEC:-./work/e2e/robin/p3/props_spec.json}; GP=${GLOWP:-0.35}
rm -rf $D; cp -a ./work/e2e/calib/proj_p2v6 $D; rm -rf $D/renders
for kv in hull_a_p2_half.png:FSB025_Bourgogne_1945_Hull_a.dds dh_a_p2_half.png:FSB025_Bourgogne_1945_DeckHouse_a.dds FGM049_a_p2_half.png:FGM049_380mm_45_Mle_1935_a.dds FGS036_a_p2_half.png:FGS036_127mm_54_5_1948_a.dds FGS053_a_p2_half.png:FGS053_152_55_Mle_1950_a.dds; do
  $W --json tex import $T/${kv%%:*} --like ${kv#*:} --project $D | grep -q '"ok": true' && echo "tex ${kv%%:*} ok" || echo "tex ${kv%%:*} FAIL"; done
$W --json glow --project $D --mg FSB025_Bourgogne_1945_Hull_mg.dds --mask $T/hull_glow.png --power $GP --mfm FSB025_Bourgogne_1945_Hull.mfm --mfm FSB025_Bourgogne_1945_Hull_skinned.mfm | grep -E '"ok"' | head -1
$W --json glow --project $D --mg FSB025_Bourgogne_1945_DeckHouse_mg.dds --mask $T/dh_glow.png --power $GP --mfm FSB025_Bourgogne_1945_DeckHouse.mfm --mfm FSB025_Bourgogne_1945_DeckHouse_skinned.mfm | grep -E '"ok"' | head -1
SPEC=$SPEC python3 - <<'P' > /tmp/a7cmds.sh
import json, os; c=json.load(open(os.environ['SPEC']))
for p in c['props']:
    verb='animate' if p.get('anim') else 'import'
    a=f"$W --json model {verb} --project $D --mesh $P/{p['name']}.glb --like '{p['like']}' --name {p['name']} --texture $P/prop_atlas.png --glow $P/prop_glow.png --power {c['power']} --tex-size 1024 --gloss {c['gloss']} --geom-dir ./work/m3/game/content/gameplay/france"
    for k in ['section','part','hp','node','at','rot','scale']:
        if p.get(k): a+=f" --{k} '{p[k]}'"
    if p.get('anim'): a+=f" --preset {p['anim']['preset']} --float-amp {p['anim']['amp']} --period {p['anim']['period']}"
    print(a+f" | grep -E '\"ok\"|\"error\"' | head -2 | tr '\\n' ' '; echo ' <- {p['name']}'")
P
source /tmp/a7cmds.sh
# T4: 炮塔壳烘焙法线/AO(与图集同 UV; 无则跳过)
if [ -f "$P/prop_n.png" ]; then
  for s in rb_shell_FGM049 rb_shell_FGS053 rb_shell_FGS036; do
    $W --json tex import "$P/prop_n.png" --like ${s}_n.dds --project $D | grep -q '"ok": true' && echo "n $s ok" || echo "n $s FAIL"
    [ -f "$P/prop_ao.png" ] && $W --json tex import "$P/prop_ao.png" --like ${s}_ao.dds --project $D | grep -q '"ok": true' && echo "ao $s ok" || true
  done
fi
# 原壳降噪(可选, PRUNE=1): 舷侧小件 + 主甲板小件(见 workflow.layers.shape.refine.clean)
if [ "${PRUNE:-0}" = 1 ]; then
  $W --json model prune --project $D --renderset '^(Bow|MidFront|MidBack|Stern)$' --facing side --max-size 0.08 --geom-dir ./work/m3/game/content/gameplay/france | grep -E '"ok"' | head -1
  $W --json model prune --project $D --renderset '^(Bow|MidFront|MidBack|Stern)$' --facing up --max-size 0.05 --box=-2,0,-10,2,0.75,10 --geom-dir ./work/m3/game/content/gameplay/france | grep -E '"ok"' | head -1
fi
