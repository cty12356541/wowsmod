#!/bin/bash
# 生成交付截图（1600×900）。需要先运行 mock/server.py（端口 8960）。
cd "$(dirname "$0")/.."
D=${D:-shots}
B=http://127.0.0.1:8960/
settle='["render",30,0.05]'
selTex() { echo '["js","document.querySelector(\"#texOnlyChanged\").checked||document.querySelector(\"#texOnlyChanged\").click()"],["js","(document.querySelector(\"[data-tex=\\\"'$1'\\\"]\")||{click(){}}).click()"],["js","(document.querySelector(\"#texChSeg [data-tch='$2']\")).click()"]'; }
run() { URL="$1" python3 tools/shot.py "$2" 2>&1 | grep -E "saved|PAGEERROR|NOT READY|js ->"; }

run "$B?lang=zh&q=high" '[["wait",1],'"$(selTex FSB025_Bourgogne_1945_Hull_a rgb)"','"$settle"',["shot","'$D'/01_real_textures_overview.png"]]'
run "$B?lang=zh&q=high" '[["js","WM.setView(\"s1\")"],["waitfn","!WM.S.loadingView"],["click","#modeSeg button[data-mode=split]"],["js","WM.V.setSplit(0.5)"],["js","WM.V.setView([8.2,2.6,-8.6],[0.2,0.9,-1.6])"],'"$(selTex FSB025_Bourgogne_1945_Hull_a rgb)"','"$settle"',["shot","'$D'/02_split_before_after_deepsea.png"]]'
run "$B?lang=zh&q=high" '[["click","#envSeg button[data-env=night]"],["js","document.querySelector(\"#hiToggle\").click()"],["js","var r=document.querySelector(\"#glowRange\");r.value=1.3;r.dispatchEvent(new Event(\"input\"))"],["js","WM.V.preset(\"funnel\",true)"],'"$(selTex FSB025_Bourgogne_1945_DeckHouse_mg b)"','"$settle"',["shot","'$D'/03_dark_studio_glow.png"]]'
run "$B?lang=zh&q=high" '[["js","WM.setView(\"s1\")"],["waitfn","!WM.S.loadingView"],["click","#modeSeg button[data-mode=side]"],["js","WM.S.filesOpen.s1=true;WM.renderAll()"],["js","WM.V.preset(\"hero\",true)"],'"$settle"',["shot","'$D'/04_timeline_review.png"]]'
run "$B?lang=ru&q=high" '[["click","#modeSeg button[data-mode=split]"],["js","WM.V.setSplit(0.42)"],["js","WM.S.collapsed[\"g:secondary\"]=false;WM.renderAll()"],["js","WM.V.preset(\"port\",true)"],'"$(selTex FSB025_Bourgogne_1945_DeckHouse_mg b)"','"$settle"',["shot","'$D'/05_russian_ui.png"]]'
run "$B?lang=zh&q=mid" '[["js","WM.setQuality(\"low\",true)"],["waitfn","!WM.S.loadingView"],["wait",1],'"$settle"',["shot","'$D'/06_low_quality.png"]]'
