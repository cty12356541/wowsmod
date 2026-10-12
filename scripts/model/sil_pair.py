#!/usr/bin/env python3
"""标杆 vs 我方同角剪影并排 + 红/青/白叠加。用法: sil_pair.py --bench a.jpg --ours b.png --out out.jpg [--title T]"""
import argparse, numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageOps
ap=argparse.ArgumentParser(); ap.add_argument('--bench',required=True); ap.add_argument('--ours',required=True)
ap.add_argument('--out',required=True); ap.add_argument('--title',default=''); ap.add_argument('--bg',type=float,default=18)
a=ap.parse_args()
def sil(path):
    im=np.asarray(Image.open(path).convert('RGB')).astype(float); c=24
    bg=np.median(np.concatenate([im[:c,:c].reshape(-1,3),im[:c,-c:].reshape(-1,3),im[-c:,:c].reshape(-1,3),im[-c:,-c:].reshape(-1,3)]),0)
    m=(np.linalg.norm(im-bg,axis=-1)>=a.bg).astype(np.uint8)*255
    return Image.fromarray(m).filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.MinFilter(3))
sb=sil(a.bench).convert('L').resize((640,360)); so=sil(a.ours).convert('L').resize((640,360))
b=np.asarray(sb)>40; o=np.asarray(so)>40
ov=np.zeros((360,640,3),np.uint8); ov[b&~o]=(220,60,60); ov[o&~b]=(40,180,220); ov[b&o]=(240,240,240)
canvas=Image.new('RGB',(1300,420),(18,16,24))
canvas.paste(ImageOps.colorize(sb,'black','#c8c8d0').convert('RGB'),(0,40))
canvas.paste(ImageOps.colorize(so,'black','#c8c8d0').convert('RGB'),(640,40))
canvas.paste(Image.fromarray(ov).resize((280,158)),(510,120))
d=ImageDraw.Draw(canvas); d.text((10,8),f'{a.title}  左标杆 右我方 中叠加(红仅标杆/青仅我方/白重叠)',fill='white')
union=(b|o).sum(); inter=(b&o).sum()
d.text((10,400),f'overlap={inter/max(union,1):.2f} ours_extra={(o&~b).sum()/max(o.sum(),1):.2f}',fill='#aaa')
canvas.save(a.out,quality=90); print({'overlap':round(inter/max(union,1),3),'ours_extra':round((o&~b).sum()/max(o.sum(),1),3)})
