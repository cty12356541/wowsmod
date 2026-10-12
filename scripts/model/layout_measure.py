#!/usr/bin/env python3
"""外带布局框量: 在同角图上标 hull/甲板/桅, 量各件 along/高出甲板/宽高比。
用法: layout_measure.py --spec measure.json --outdir layout/
measure.json: [{"name","path","hull":[x0,y0,x1,y1],"deck_y","mast_top","parts":[[label,x0,y0,x1,y1],...]}]
输出: 标注 jpg + measure.json(相对船长/舷楼高)。见 skill shape.refine.layout。
"""
import argparse, json
from pathlib import Path
from PIL import Image, ImageDraw
ap=argparse.ArgumentParser(); ap.add_argument('--spec',required=True); ap.add_argument('--outdir',required=True)
a=ap.parse_args(); specs=json.loads(Path(a.spec).read_text()); out=Path(a.outdir); out.mkdir(parents=True,exist_ok=True)
res=[]
for s in specs:
    im=Image.open(s['path']).convert('RGB'); d=ImageDraw.Draw(im)
    hx0,hy0,hx1,hy1=s['hull']; L=hx1-hx0; deck=s['deck_y']; mast=s['mast_top']; sh=deck-mast
    d.rectangle([hx0,hy0,hx1,hy1],outline=(0,255,0),width=2)
    d.line([(hx0,deck),(hx1,deck)],fill=(0,200,255),width=2)
    d.line([(hx0,mast),(hx1,mast)],fill=(255,200,0),width=1)
    parts=[]
    for i,(lab,x0,y0,x1,y1) in enumerate(s['parts']):
        cx,cy=(x0+x1)/2,(y0+y1)/2; col=(255,80+30*(i%5),80)
        d.rectangle([x0,y0,x1,y1],outline=col,width=2)
        row=dict(label=lab,along_hull=round((cx-hx0)/L,2),h_above_deck=round((deck-cy)/sh,2) if sh else 0,
                 w_over_L=round((x1-x0)/L,2),h_over_super=round((y1-y0)/sh,2) if sh else 0,box=[x0,y0,x1,y1])
        d.text((x0+2,y0+2),f"{lab} L={row['along_hull']} H={row['h_above_deck']} w={row['w_over_L']}",fill=col)
        parts.append(row)
    path=out/f"{s['name']}.jpg"; im.save(path,quality=88)
    res.append(dict(name=s['name'],path=str(path),hull_len_px=L,super_h_px=sh,parts=parts))
json.dump(res,open(out/'measure.json','w'),ensure_ascii=False,indent=1); print(json.dumps(res,ensure_ascii=False)[:500])
