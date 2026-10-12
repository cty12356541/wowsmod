#!/usr/bin/env python3
"""船体遮罩(给 role_share --mask): GrabCut 以矩形初始化, 排除框强制为背景(立绘/台标/UI)。
用法: ship_mask.py IMG --rect x0 y0 x1 y1 [--exclude x0 y0 x1 y1 ...] [--below y](水线以下强制背景) -o mask.png [--overlay ov.jpg]"""
import argparse, numpy as np, cv2
ap=argparse.ArgumentParser(); ap.add_argument('img'); ap.add_argument('--rect',nargs=4,type=int); ap.add_argument('--poly',help='x,y;x,y;... 多边形初始化(比矩形准, 背景复杂的标杆帧用)')
ap.add_argument('--exclude',nargs=4,type=int,action='append',default=[]); ap.add_argument('-o',required=True); ap.add_argument('--overlay'); ap.add_argument('--iter',type=int,default=6)
a=ap.parse_args(); im=cv2.imread(a.img); H,W=im.shape[:2]
m=np.full((H,W),cv2.GC_BGD,np.uint8)
if a.poly: cv2.fillPoly(m,[np.array([[int(v) for v in q.split(',')] for q in a.poly.split(';')],np.int32)],int(cv2.GC_PR_FGD))
else: x0,y0,x1,y1=a.rect; m[y0:y1,x0:x1]=cv2.GC_PR_FGD
for e in a.exclude: m[e[1]:e[3],e[0]:e[2]]=cv2.GC_BGD
bg=np.zeros((1,65)); fg=np.zeros((1,65))
cv2.grabCut(im,m,None,bg,fg,a.iter,cv2.GC_INIT_WITH_MASK)
out=np.where((m==cv2.GC_FGD)|(m==cv2.GC_PR_FGD),255,0).astype(np.uint8)
n,lab,st,_=cv2.connectedComponentsWithStats(out)
if n>1: k=1+np.argmax(st[1:,cv2.CC_STAT_AREA]); out=np.where(lab==k,255,0).astype(np.uint8)   # 只留最大连通块(船)
cv2.imwrite(a.o,out)
if a.overlay:
    ov=im.copy(); ov[out==0]=(ov[out==0]*0.25).astype(np.uint8); cv2.imwrite(a.overlay,ov)
print(dict(ok=True,frac=round(float((out>0).mean()),3)))
