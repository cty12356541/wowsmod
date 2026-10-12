#!/usr/bin/env python3
"""标杆帧 vs 我方同角出图: 并排 + 双方 role_share 条 + 分数栏。
用法: bench_pair.py --pairs pairs.json --out sheet.jpg
pairs.json: [{"name":"舷斜高","bench":"d_018.jpg","ours":"p2_day_fq_high.png","bench_box":[..]?,"ours_box":[..]?,
              "bench_hue":[260,300],"ours_hue":[260,320],"score":{"配色":3,"图案":3,"部件覆盖":3},"note":"..."}]
"""
import argparse, json, subprocess, sys
from pathlib import Path
from PIL import Image, ImageDraw
sys.path.insert(0,str(Path(__file__).parent)); import cjkfont
HERE=Path(__file__).parent
ROLE_COL={'白底':'#E8E8EE','主色块':'#6A6AD0','暗色':'#1C2040','发光线':'#9FE8F0','暖点缀':'#E0B060','中灰_未归类':'#808080'}
def role(img,hue,box,bg):
    cmd=[sys.executable,str(HERE/'role_share.py'),img,'--main-hue',str(hue[0]),str(hue[1]),'--bg-auto',str(bg)]
    if box: cmd+=['--box',*map(str,box)]
    return json.loads(subprocess.check_output(cmd).decode())
def bar(d,x,y,w,roles):
    cx=x
    for k,v in roles.items():
        ww=int(w*v); d.rectangle([cx,y,cx+ww,y+16],fill=ROLE_COL[k]); cx+=ww
ap=argparse.ArgumentParser(); ap.add_argument('--pairs',required=True); ap.add_argument('--out',required=True); ap.add_argument('--bg',type=float,default=14)
a=ap.parse_args(); pairs=json.loads(Path(a.pairs).read_text()); base=Path(a.pairs).parent
W,H=720,405; f=cjkfont.font(18); fs=cjkfont.font(14)
sheet=Image.new('RGB',(W*2+20,len(pairs)*(H+92)+40),(20,18,26)); d=ImageDraw.Draw(sheet)
d.text((10,8),'同角并排: 左=标杆(夙岚 布洛妮娅) 右=知更鸟 P2   条=role_share(白/主色/暗/发光/暖/中灰)',font=f,fill='white')
res=[]
for i,p in enumerate(pairs):
    y=40+i*(H+92)
    rs=[]
    for j,(k,hk,bk) in enumerate([('bench','bench_hue','bench_box'),('ours','ours_hue','ours_box')]):
        path=str((base/p[k]).resolve()) if not Path(p[k]).is_absolute() else p[k]
        im=Image.open(path).convert('RGB'); im=im.resize((W,int(im.height*W/im.width))).crop((0,0,W,H)); sheet.paste(im,(j*(W+20),y))
        r=role(path,p.get(hk,[260,300]),p.get(bk),p.get(k+'_bg',p.get('bg',a.bg))); rs.append(r)
        bar(d,j*(W+20),y+H+4,W,r['roles'])
        d.text((j*(W+20),y+H+24),' '.join(f"{k2}{int(v*100)}" for k2,v in r['roles'].items())+f"  L{r['L_p5']:.0f}-{r['L_p95']:.0f}",font=fs,fill=(220,220,220))
    sc=p.get('score',{}); d.text((10,y+H+46),f"[{p['name']}] "+' '.join(f'{k}:{v}' for k,v in sc.items())+'  '+p.get('note',''),font=f,fill=(255,220,120))
    res.append(dict(name=p['name'],bench=rs[0],ours=rs[1],score=sc))
sheet.save(a.out,quality=88); Path(a.out).with_suffix('.json').write_text(json.dumps(res,ensure_ascii=False,indent=1)); print(a.out)
