#!/usr/bin/env python3
"""按色彩角色(底白/主色块/暗色/发光线/中灰/暖点缀)统计面积占比。
用法: role_share.py IMG --box x0 y0 x1 y1 [--exclude x0 y0 x1 y1 ...] --main-hue 260 300 [--glow-hue 190 250] [--accent-hue 40 100]
中灰(=未设计原版灰候选)占比高 → 完成度低。"""
import argparse, json, sys
from pathlib import Path
import numpy as np
from PIL import Image
sys.path.insert(0,str(Path(__file__).parent)); from part_colors import srgb2lab
ap=argparse.ArgumentParser(); ap.add_argument('img'); ap.add_argument('--box',nargs=4,type=int)
ap.add_argument('--exclude',nargs=4,type=int,action='append',default=[])
ap.add_argument('--main-hue',nargs=2,type=float,default=[260,300]); ap.add_argument('--glow-hue',nargs=2,type=float,default=[190,250])
ap.add_argument('--accent-hue',nargs=2,type=float,default=[40,100]); ap.add_argument('--json',action='store_true')
a=ap.parse_args()
im=np.asarray(Image.open(a.img).convert('RGB')).astype(float); m=np.ones(im.shape[:2],bool)
if a.box: x0,y0,x1,y1=a.box; mm=np.zeros_like(m); mm[y0:y1,x0:x1]=1; m&=mm
for x0,y0,x1,y1 in a.exclude: m[y0:y1,x0:x1]=0
lab=srgb2lab(im[m]); L,A,B=lab[:,0],lab[:,1],lab[:,2]; C=np.hypot(A,B); H=(np.degrees(np.arctan2(B,A))+360)%360
def inh(r): lo,hi=r; return (H>=lo)&(H<=hi) if lo<=hi else (H>=lo)|(H<=hi)
roles={}
glow=(L>=78)&(C>=10)&inh(a.glow_hue)
white=(L>=70)&(C<10)&~glow
accent=(C>=25)&inh(a.accent_hue)&~glow
main=(C>=22)&inh(a.main_hue)&~glow&~accent
dark=(L<30)&~main&~accent
mid=~(glow|white|accent|main|dark)
n=len(L)
for k,v in dict(白底=white,主色块=main,暗色=dark,发光线=glow,暖点缀=accent,中灰_未归类=mid).items(): roles[k]=round(float(v.sum())/n,3)
res=dict(img=a.img,roles=roles,L_p5=round(float(np.percentile(L,5)),1),L_p95=round(float(np.percentile(L,95)),1))
print(json.dumps(res,ensure_ascii=False))
