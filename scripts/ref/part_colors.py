#!/usr/bin/env python3
"""Part-wise color extraction (角色本色 / 标杆分区取色).

extract: spec json {"image":..., "crop":[x0,y0,x1,y1]?, "parts":{name:[x0,y0,x1,y1],...}}
  -> per part k-means (k=3) dominant + secondary, Lab L*, chroma, hue; board png + json.
derive: mapping json {"zones":[{"zone":..,"part":..,"pick":"dominant|secondary|#hex","share":0.3,"why":..}],
  "parts_json": out of extract} -> derivation board (立绘取色 → 分区映射) + scheme json.
Boxes are in coordinates of the (optionally pre-cropped) image.
"""
import argparse, json, colorsys, math
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
import numpy as np

def font(sz):
    import sys; sys.path.insert(0, str(Path(__file__).parent)); import cjkfont
    fp=cjkfont.font_path()
    return ImageFont.truetype(fp, sz) if fp else ImageFont.load_default()

def srgb2lab(rgb):
    c=np.asarray(rgb,float)/255; c=np.where(c>0.04045,((c+0.055)/1.055)**2.4,c/12.92)
    M=np.array([[0.4124,0.3576,0.1805],[0.2126,0.7152,0.0722],[0.0193,0.1192,0.9505]])
    xyz=c@M.T/np.array([0.95047,1,1.08883]); f=np.where(xyz>0.008856,np.cbrt(xyz),7.787*xyz+16/116)
    L=116*f[...,1]-16; a=500*(f[...,0]-f[...,1]); b=200*(f[...,1]-f[...,2]); return np.stack([L,a,b],-1)

def kmeans(px,k=3,it=20,seed=0):
    rng=np.random.default_rng(seed); px=px.astype(float)
    if len(px)>20000: px=px[rng.choice(len(px),20000,replace=False)]
    cen=px[rng.choice(len(px),k,replace=False)]
    for _ in range(it):
        d=((px[:,None]-cen[None])**2).sum(-1); lab=d.argmin(1)
        for j in range(k):
            if (lab==j).any(): cen[j]=px[lab==j].mean(0)
    cnt=np.bincount(lab,minlength=k)/len(lab); o=np.argsort(-cnt); return cen[o],cnt[o]

def desc(rgb):
    L,a,b=srgb2lab(rgb); h=(math.degrees(math.atan2(b,a))+360)%360; C=math.hypot(a,b)
    hh,_,_=colorsys.rgb_to_hsv(*(np.array(rgb)/255))
    return dict(hex='#%02X%02X%02X'%tuple(int(round(v)) for v in rgb),L=round(float(L),1),C=round(float(C),1),
                hue_lab=round(h),hue_hsv=round(hh*360))

def extract(a):
    spec=json.loads(Path(a.spec).read_text())
    im=Image.open(spec['image']).convert('RGBA')
    if spec.get('crop'): im=im.crop(tuple(spec['crop']))
    out={'image':spec['image'],'crop':spec.get('crop'),'parts':{}}
    tiles=[]
    for name,box in spec['parts'].items():
        c=im.crop(tuple(box)); arr=np.asarray(c); px=arr[...,:3][arr[...,3]>128].reshape(-1,3)
        cen,cnt=kmeans(px,a.k)
        sw=[dict(desc(cen[i]),share=round(float(cnt[i]),3)) for i in range(len(cen))]
        out['parts'][name]={'box':box,'swatches':sw,'note':spec.get('notes',{}).get(name,'')}
        tiles.append((name,c.convert('RGB'),sw))
    W,H=230,300; f=font(16); fs=font(13)
    cols=min(5,len(tiles)); rows=(len(tiles)+cols-1)//cols
    board=Image.new('RGB',(cols*W,rows*H+40),(24,22,30)); d=ImageDraw.Draw(board)
    d.text((8,8),spec.get('title','部位取色'),font=font(20),fill='white')
    for i,(n,c,sw) in enumerate(tiles):
        x=(i%cols)*W+6; y=(i//cols)*H+40; t=c.copy(); t.thumbnail((W-12,150)); board.paste(t,(x,y+20))
        d.text((x,y),n,font=f,fill=(255,230,120))
        for j,s in enumerate(sw):
            yy=y+175+j*40; d.rectangle([x,yy,x+50,yy+34],fill=s['hex'])
            d.text((x+56,yy),f"{s['hex']} {int(s['share']*100)}%",font=fs,fill='white')
            d.text((x+56,yy+17),f"L{s['L']:.0f} C{s['C']:.0f} h{s['hue_lab']}",font=fs,fill=(190,190,190))
    board.save(a.out_prefix+'_board.png'); Path(a.out_prefix+'_parts.json').write_text(json.dumps(out,ensure_ascii=False,indent=1))
    print(a.out_prefix+'_board.png')

def pick(parts,ref):
    if ref.startswith('#'): return ref.upper()
    p,_,idx=ref.partition(':'); return parts[p]['swatches'][int(idx or 0)]['hex']

def derive(a):
    m=json.loads(Path(a.mapping).read_text()); parts={}
    for pj in m['parts_json']: parts.update(json.loads(Path(pj).read_text())['parts'])
    rows=[]
    for z in m['zones']:
        src=pick(parts,z['src']); tgt=z.get('final',src)
        rows.append(dict(z,src_hex=src,final=tgt.upper(),src_desc=desc(tuple(int(src[i:i+2],16) for i in (1,3,5))),
                         final_desc=desc(tuple(int(tgt[i:i+2],16) for i in (1,3,5)))))
    W=1300; RH=64; f=font(17); fs=font(14)
    board=Image.new('RGB',(W,90+RH*len(rows)+120),(24,22,30)); d=ImageDraw.Draw(board)
    d.text((10,10),m.get('title','色板推导'),font=font(22),fill='white')
    d.text((10,44),'角色部位(立绘取色)        →  修正后终色      →  船体分区 / 面积占比 / 理由',font=fs,fill=(200,200,200))
    for i,r in enumerate(rows):
        y=80+i*RH
        d.rectangle([10,y,70,y+50],fill=r['src_hex']); d.text((78,y+4),r['src'],font=fs,fill=(255,230,120)); d.text((78,y+24),f"{r['src_hex']} L{r['src_desc']['L']:.0f}",font=fs,fill='white')
        d.text((300,y+14),'→',font=f,fill='white')
        d.rectangle([330,y,420,y+50],fill=r['final']); d.text((428,y+4),r['final'],font=fs,fill='white'); d.text((428,y+24),f"L{r['final_desc']['L']:.0f} C{r['final_desc']['C']:.0f}",font=fs,fill=(190,190,190))
        d.text((540,y+4),f"{r['zone']}  [{int(r.get('share',0)*100)}%]",font=f,fill=(150,220,255)); d.text((540,y+28),r.get('why','')[:62],font=fs,fill=(210,210,210))
    # area bar
    y=90+RH*len(rows)+10; x=10; tot=sum(r.get('share',0) for r in rows) or 1
    d.text((10,y),'面积占比条 (按标杆分区逻辑)',font=fs,fill='white')
    for r in rows:
        w=int((W-20)*r.get('share',0)/tot); d.rectangle([x,y+24,x+w,y+84],fill=r['final']); x+=w
    board.save(a.out_prefix+'_derive.png')
    Ls=[r['final_desc']['L'] for r in rows if r.get('share',0)>=0.03]
    res=dict(title=m.get('title'),rows=rows,L_span=round(max(Ls)-min(Ls),1))
    Path(a.out_prefix+'_scheme.json').write_text(json.dumps(res,ensure_ascii=False,indent=1)); print(a.out_prefix+'_derive.png', 'L_span',res['L_span'])

def main():
    ap=argparse.ArgumentParser(); sp=ap.add_subparsers(dest='cmd',required=True)
    e=sp.add_parser('extract'); e.add_argument('spec'); e.add_argument('--k',type=int,default=3); e.add_argument('--out-prefix',required=True); e.set_defaults(fn=extract)
    r=sp.add_parser('derive'); r.add_argument('mapping'); r.add_argument('--out-prefix',required=True); r.set_defaults(fn=derive)
    a=ap.parse_args(); a.fn(a)

if __name__=='__main__':
    main()
