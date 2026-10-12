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
ap.add_argument('--accent-hue',nargs=2,type=float,default=[40,100]); ap.add_argument('--json',action='store_true'); ap.add_argument('--split-main',type=float,default=0,help='主色块按 L* 拆浅/深(如 55: 浅=periwinkle 辅色, 深=royal 点缀)'); ap.add_argument('--mask',help='船体遮罩 PNG(白=船), 与 --box 取交')
ap.add_argument('--bg-auto',type=float,default=0,help='按画面四边取背景色(k=4), ΔE<该值的像素剔除(水/天/棚)')
ap.add_argument('--bg-corners',action='store_true',help='背景色改取四角 24px 块的中位色(船体贴边时 --bg-auto 的四边 k-means 会把船色当背景剔除)')
a=ap.parse_args()
im=np.asarray(Image.open(a.img).convert('RGB')).astype(float); m=np.ones(im.shape[:2],bool)
if a.box: x0,y0,x1,y1=a.box; mm=np.zeros_like(m); mm[y0:y1,x0:x1]=1; m&=mm
for x0,y0,x1,y1 in a.exclude: m[y0:y1,x0:x1]=0
if a.mask: m&=np.asarray(Image.open(a.mask).convert('L').resize((im.shape[1],im.shape[0])))>127
if a.bg_auto>0:
    from part_colors import kmeans
    Hh,Ww=im.shape[:2]; bd=np.concatenate([im[:12].reshape(-1,3),im[-12:].reshape(-1,3),im[:,:12].reshape(-1,3),im[:,-12:].reshape(-1,3)])
    if a.bg_corners:
        c=24; cen=np.array([np.median(b.reshape(-1,3),0) for b in (im[:c,:c],im[:c,-c:],im[-c:,:c],im[-c:,-c:],im[:c,Ww//2-c:Ww//2+c],im[-c:,Ww//2-c:Ww//2+c])])
    else:
        cen,_=kmeans(bd,4)
    cl=srgb2lab(cen); full=srgb2lab(im)
    dmin=np.min(np.linalg.norm(full[...,None,:]-cl[None,None],axis=-1),axis=-1)
    m&=dmin>=a.bg_auto
lab=srgb2lab(im[m]); L,A,B=lab[:,0],lab[:,1],lab[:,2]; C=np.hypot(A,B); H=(np.degrees(np.arctan2(B,A))+360)%360
def inh(r): lo,hi=r; return (H>=lo)&(H<=hi) if lo<=hi else (H>=lo)|(H<=hi)
roles={}
glow=(L>=78)&(C>=10)&inh(a.glow_hue)
dark=(L<32)
white=(L>=70)&(C<14)&~glow
accent=(C>=25)&inh(a.accent_hue)&~glow&~dark
main=(C>=20)&inh(a.main_hue)&~glow&~accent&~dark
mid=~(glow|white|accent|main|dark)
n=len(L)
for k,v in dict(白底=white,主色块=main,暗色=dark,发光线=glow,暖点缀=accent,中灰_未归类=mid).items(): roles[k]=round(float(v.sum())/n,3)
if a.split_main>0: roles['主色块_浅']=round(float((main&(L>=a.split_main)).sum())/n,3); roles['主色块_深']=round(float((main&(L<a.split_main)).sum())/n,3)
res=dict(img=a.img,ship_px=int(n),roles=roles,L_p5=round(float(np.percentile(L,5)),1),L_p95=round(float(np.percentile(L,95)),1))
print(json.dumps(res,ensure_ascii=False))
