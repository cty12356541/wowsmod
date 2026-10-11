#!/usr/bin/env python3
"""Count triangles in glTF/GLB props; fail if over budget.
Usage: mesh_budget.py [--budget-each 12000] [--budget-total 40000] a.glb b.glb
Exit 1 if any over. Needs pygltflib or falls back to rough JSON parse for .gltf.
"""
import argparse, json, struct, sys
from pathlib import Path

def tris_glb(path):
    data=Path(path).read_bytes()
    if data[:4]!=b'glTF': raise ValueError('not glb')
    # GLB: 12 byte header, then JSON chunk, then BIN
    jlen=struct.unpack_from('<I',data,12)[0]
    # chunk0 length at 12, type at 16
    ctype=data[16:20]
    off=20
    if ctype!=b'JSON':
        # sometimes padding; find JSON
        raise ValueError('chunk0 not JSON')
    j=json.loads(data[off:off+jlen])
    return tris_from_gltf(j)

def tris_from_gltf(j):
    acc=j.get('accessors',[]); total=0; per=[]
    for mi,m in enumerate(j.get('meshes',[])):
        t=0
        for p in m.get('primitives',[]):
            if 'indices' in p:
                a=acc[p['indices']]; t+=a.get('count',0)//3
            else:
                # non-indexed: POSITION count/3
                pos=p.get('attributes',{}).get('POSITION')
                if pos is not None: t+=acc[pos].get('count',0)//3
        per.append((m.get('name',f'mesh{mi}'),t)); total+=t
    return total, per

def tris_any(path):
    p=Path(path)
    if p.suffix.lower()=='.glb': return tris_glb(p)
    if p.suffix.lower()=='.gltf': return tris_from_gltf(json.loads(p.read_text()))
    raise ValueError(f'unsupported {p.suffix}')

ap=argparse.ArgumentParser(); ap.add_argument('files',nargs='+')
ap.add_argument('--budget-each',type=int,default=12000); ap.add_argument('--budget-total',type=int,default=40000)
a=ap.parse_args(); grand=0; bad=False
for f in a.files:
    try:
        tot,per=tris_any(f)
    except Exception as e:
        print(f'FAIL {f}: {e}'); bad=True; continue
    grand+=tot; flag='OK' if tot<=a.budget_each else 'OVER'
    if flag=='OVER': bad=True
    print(f'{flag} {f}: {tot} tris (budget_each={a.budget_each})')
    for n,t in per: print(f'  - {n}: {t}')
print(f'TOTAL {grand} (budget_total={a.budget_total})', 'OK' if grand<=a.budget_total else 'OVER')
if grand>a.budget_total: bad=True
sys.exit(1 if bad else 0)
