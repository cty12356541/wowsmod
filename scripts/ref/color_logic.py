#!/usr/bin/env python3
"""配色逻辑工具: 量化色板 + 深/中/浅角色 + 船体分区预览条。

  python color_logic.py analyze <图>... [--k 8] [--out-prefix book/ex]
  python color_logic.py scheme --name P1 --out book/scheme_p1.png \\
      --hull '#F5F0E6' --band '#B8C8E8' --deckhouse '#D4B5E8' --parts '#F5F0E6' \\
      --accent '#E8D4A0' --line '#4A3D6B' --glow '#C8D8F0' \\
      [--port-albedo ./target/release/wowsmod]

analyze: 写 <prefix>_swatch.png + <prefix>_logic.json(占比/L*/sat/深中浅建议)
scheme:  画分区示意条(船壳/色带/上层/炮塔/点缀/结构线/发光) + 可选 port-albedo 换算表
"""
import argparse, json, subprocess, sys
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFont

def load_rgb(paths, k, bg='auto'):
    # inline minimal kmeans (same spirit as palette.py)
    from importlib.util import spec_from_file_location, module_from_spec
    # reuse palette helpers by duplicating small core
    xs=[]
    for pth in paths:
        im=Image.open(pth).convert('RGBA'); im.thumbnail((512,512))
        a=np.asarray(im).reshape(-1,4).astype(np.float32)
        rgb=a[a[:,3]>128,:3].copy()
        if bg=='auto' and len(rgb):
            c=np.asarray(im)[[0,0,-1,-1],[0,-1,0,-1],:3].astype(np.float32)
            ref=np.median(c,0)
            if np.ptp(c,0).max()<30:
                rgb=rgb[np.linalg.norm(rgb-ref,axis=1)>30]
        if len(rgb):
            xs.append(rgb.reshape(-1,3))
    if not xs:
        raise SystemExit('no pixels')
    x=np.concatenate(xs, axis=0)
    if len(x)>200000:
        x=x[np.random.default_rng(1).choice(len(x),200000,False)]
    rng=np.random.default_rng(0); c=x[rng.choice(len(x),min(k,len(x)),False)]
    for _ in range(25):
        d=((x[:,None,:]-c[None])**2).sum(-1); lab=d.argmin(1)
        for j in range(len(c)):
            if (lab==j).any(): c[j]=x[lab==j].mean(0)
    n=np.bincount(lab,minlength=len(c))
    rows=[]
    for j in np.argsort(-n):
        if n[j]==0: continue
        col=[int(round(v)) for v in c[j]]
        mx,mn=max(col),min(col)
        r,g,b=[v/255 for v in col]
        f=lambda u: u/12.92 if u<=0.04045 else ((u+0.055)/1.055)**2.4
        y=0.2126*f(r)+0.7152*f(g)+0.0722*f(b)
        L=116*(y**(1/3) if y>0.008856 else 7.787*y+16/116)-16
        # hue approx
        import colorsys
        h,s,v=colorsys.rgb_to_hsv(col[0]/255,col[1]/255,col[2]/255)
        rows.append({'hex':'#%02X%02X%02X'%tuple(col),'pct':round(100*n[j]/n.sum(),1),
                     'L':round(L,1),'sat':round((mx-mn)/mx,2) if mx else 0,
                     'hue_deg':round(h*360,1)})
    # assign deep/mid/light by L*
    Ls=sorted(rows, key=lambda r:r['L'])
    for i,r in enumerate(Ls):
        if i < max(1,len(Ls)//3): r['value_role']='deep'
        elif i >= len(Ls)-max(1,len(Ls)//3): r['value_role']='light'
        else: r['value_role']='mid'
    # area roles by pct
    by_pct=sorted(rows,key=lambda r:-r['pct'])
    if by_pct: by_pct[0]['area_role']='primary'
    if len(by_pct)>1: by_pct[1]['area_role']='secondary'
    for r in by_pct[2:4]: r['area_role']=r.get('area_role') or 'accent'
    for r in rows:
        r.setdefault('area_role','accent')
    return rows

def swatch(rows, path, title=''):
    W=110; H=160+ (18 if title else 0)
    im=Image.new('RGB',(W*len(rows),H),'white'); d=ImageDraw.Draw(im)
    y0=18 if title else 0
    if title: d.text((6,2), title, fill='black')
    for i,r in enumerate(rows):
        d.rectangle([i*W,y0,i*W+W-2,y0+90], fill=r['hex'])
        d.text((i*W+4,y0+94), f"{r['hex']}\n{r['pct']}% L{r['L']:.0f}\n{r['value_role']}/{r['area_role'][:3]}", fill='black')
    im.save(path); return path

def scheme_board(args, port_map):
    zones=[
        ('船壳大面 hull', args.hull, 'light/primary'),
        ('干舷/浪带 band', args.band, 'mid/secondary'),
        ('上层 deckhouse', args.deckhouse, 'mid'),
        ('炮塔 parts', args.parts, 'light'),
        ('点缀 accent', args.accent, 'accent'),
        ('结构线 line', args.line, 'deep'),
        ('发光 glow', args.glow, 'accent-hi'),
    ]
    im=Image.new('RGB',(900, 80+70*len(zones)+40),'#1a1a1a'); d=ImageDraw.Draw(im)
    d.text((12,10), f"scheme {args.name}  (screen colors; tex via port-albedo)", fill='white')
    y=36
    for name,col,role in zones:
        d.rectangle([12,y,120,y+52], fill=col)
        tex=port_map.get(col.upper(), port_map.get(col, '?'))
        d.text((132,y+4), f"{name}\n screen {col}  → tex {tex}\n role {role}", fill='white')
        y+=62
    im.save(args.out); return args.out

def port_albedo(exe, color):
    if not exe: return color
    try:
        o=subprocess.check_output([exe,'port-albedo',color,'--json'], text=True)
        return json.loads(o).get('to_color', color)
    except Exception:
        return color

def main():
    ap=argparse.ArgumentParser()
    sub=ap.add_subparsers(dest='cmd', required=True)
    a=sub.add_parser('analyze'); a.add_argument('images',nargs='+'); a.add_argument('--k',type=int,default=8)
    a.add_argument('--out-prefix',required=True); a.add_argument('--title',default='')
    s=sub.add_parser('scheme'); s.add_argument('--name',required=True); s.add_argument('--out',required=True)
    for k in ('hull','band','deckhouse','parts','accent','line','glow'):
        s.add_argument('--'+k, required=True)
    s.add_argument('--port-albedo', default='')
    args=ap.parse_args()
    if args.cmd=='analyze':
        rows=load_rgb(args.images, args.k)
        pref=Path(args.out_prefix)
        pref.parent.mkdir(parents=True, exist_ok=True)
        sp=str(pref)+'_swatch.png'
        swatch(rows, sp, args.title or pref.name)
        # logic summary
        lights=[r for r in rows if r['value_role']=='light']
        deeps=[r for r in rows if r['value_role']=='deep']
        Lspan=(max(r['L'] for r in rows)-min(r['L'] for r in rows)) if rows else 0
        hues=[r['hue_deg'] for r in rows if r['sat']>0.15]
        harmony='unknown'
        if len(hues)>=2:
            span=min((max(hues)-min(hues))%360, (min(hues)+360-max(hues))%360)
            if span<60: harmony='analogous(~邻近)'
            elif 150<=span<=210: harmony='complementary(~互补)'
            elif 90<=span<150: harmony='split/contrast'
            else: harmony=f'spread({span:.0f}°)'
        logic={'colors':rows,'L_span':round(Lspan,1),'harmony_guess':harmony,
               'suggest_zones':{
                   'hull_light': (lights[0]['hex'] if lights else None),
                   'structure_deep': (deeps[0]['hex'] if deeps else None),
                   'primary_area': next(r['hex'] for r in rows if r['area_role']=='primary'),
               },
               'pass_L25': Lspan>=25}
        json.dump(logic, open(str(pref)+'_logic.json','w',encoding='utf-8'), ensure_ascii=False, indent=2)
        print(json.dumps({'swatch':sp,'logic':str(pref)+'_logic.json','L_span':Lspan,'harmony':harmony,'pass_L25':Lspan>=25},ensure_ascii=False))
    else:
        pm={}
        for c in (args.hull,args.band,args.deckhouse,args.parts,args.accent,args.line,args.glow):
            pm[c]=port_albedo(args.port_albedo, c)
        scheme_board(args, pm)
        json.dump({'name':args.name,'screen_to_tex':pm,'out':args.out}, open(Path(args.out).with_suffix('.json'),'w'), ensure_ascii=False, indent=2)
        print(json.dumps({'out':args.out,'map':pm},ensure_ascii=False))

if __name__=='__main__':
    main()
