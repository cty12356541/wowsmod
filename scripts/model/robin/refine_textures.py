#!/usr/bin/env python3
"""Reproducible Robin texture cleanup. Mutates only an independent staging copy.

Requires numpy, scipy and Pillow. The supplied current DDS is authoritative;
game albedo is used only to invert the documented legacy luminance factor.
Run prepare, process, import, guard, mipguard, verify, secondary, weather-final.
"""
from __future__ import annotations
import argparse, hashlib, io, json, re, shutil, struct, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
from functools import lru_cache
from pathlib import Path
import numpy as np
from PIL import Image, ImageFilter, ImageDraw
from scipy import ndimage as ndi
from scipy import sparse
from scipy.sparse.linalg import spsolve

ROOT = Path(__file__).resolve().parents[3]
SOURCE = Path('E:/WoWSModding/out/compat-26.11-20261113/candidate-project')
STAGE = Path('E:/WoWSModding/out/robin-refine-20261116/texture-stage')
AUDIT = ROOT / 'out/robin-refine-20261116/audit/textures'
OUT = ROOT / 'out/robin-refine-20261116/textures'
EXE = ROOT / 'out/delivery/mod-library-20261113/wowsmod.exe'
NAMES = {'hull':'FSB025_Bourgogne_1945_Hull', 'dh':'FSB025_Bourgogne_1945_DeckHouse',
         'fgm':'FGM049_380mm_45_Mle_1935', 'fgs036':'FGS036_127mm_54_5_1948',
         'fgs053':'FGS053_152_55_Mle_1950', 'barrel':'C035_Gun_Barrel',
         'shell049':'rb_shell_FGM049','shell036':'rb_shell_FGS036','shell053':'rb_shell_FGS053'}

def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def dump(p, value):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(value, indent=2, ensure_ascii=False), encoding='utf-8')
def cli(*args):
    cmd = [str(EXE), '--game', 'E:/Korabli', '--json', *map(str,args)]
    r = subprocess.run(cmd, capture_output=True, text=True, encoding='utf-8')
    with (OUT/'cli-log.jsonl').open('a',encoding='utf-8') as f:
        f.write(json.dumps({'args':cmd,'code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},ensure_ascii=False)+'\n')
    if r.returncode: raise RuntimeError(r.stderr or r.stdout)
    return json.loads(r.stdout)
def image(path): return np.asarray(Image.open(path).convert('RGBA')).copy()
def save(path, a): Image.fromarray(np.clip(np.rint(a),0,255).astype('uint8')).save(path)
def luma(a): return np.asarray(Image.fromarray(a[...,:3].astype('uint8')).convert('L'), dtype=np.float32)
def blur(a, sigma): return ndi.gaussian_filter(a.astype(np.float32), sigma, mode='nearest')
def dilate(a, n=2): return ndi.maximum_filter(a, size=2*n+1)
def soften(a, n=3): return np.clip(ndi.distance_transform_edt(a)/n,0,1).astype(np.float32)
def source_path(key, channel):
    paths=list((SOURCE/'assets').rglob(NAMES[key]+f'_{channel}.dds'))
    assert len(paths)==1, (key,channel,paths)
    return paths[0]
def raw(key,channel, game=False):
    suffix='_game' if game else ''
    p=OUT/'inputs'/f'{key}_{channel}{suffix}.png'
    if not p.exists():
        existing=AUDIT/p.name
        if existing.exists(): shutil.copy2(existing,p)
        else: cli('tex','export',NAMES[key]+f'_{channel}.dds','--project',SOURCE,'-o',p,*(['--from-game'] if game else []))
    return image(p)

def prepare():
    if not STAGE.exists():
        shutil.copytree(SOURCE,STAGE,ignore=shutil.ignore_patterns('.install','.wowsmod'))
    else:
        marker=OUT/'source-manifest.json'
        if not marker.exists(): raise RuntimeError('Refuse existing unowned stage')
    manifest={str(p.relative_to(SOURCE)).replace('\\','/'):{'sha256':sha(p),'bytes':p.stat().st_size}
              for p in (SOURCE/'assets').rglob('*') if p.is_file()}
    prior=OUT/'source-manifest.json'
    if prior.exists() and json.loads(prior.read_text(encoding='utf-8'))!=manifest:
        raise RuntimeError('Source identity changed; use a new independent evidence/staging destination')
    dump(OUT/'source-manifest.json',manifest)
    for key in NAMES:
        for channel in ['a','n','mg','ao']: raw(key,channel)
        if not key.startswith('shell'): raw(key,'a',True)
    # Full-resolution UV classification, not a resized historical paint mask.
    for key in ['hull','dh']:
        p=OUT/'inputs'/f'{key}_pos_4096.npz'
        if not p.exists():
            cmd=[sys.executable,str(ROOT/'scripts/paint/uvpos_bake.py'),str(SOURCE/'.wowsmod/cache/geometry.glb'),
                 '--material',NAMES[key]+r'(_skinned)?\.mfm$','--size','4096','-o',str(p)]
            subprocess.run(cmd,check=True)

def regions(key, a, mg, ao):
    h,w=a.shape[:2]
    if key in ('hull','dh'):
        z=np.load(OUT/'inputs'/f'{key}_pos_4096.npz')
        cov=z['cov']>0; nrm=z['nrm']; pos=z['pos']
        deck=cov & ((z['upc']>0)|(z['wrapc']>0)|(nrm[...,1]>.65))
        if key=='dh': deck &= pos[...,1]<.65
        cover=dilate(cov,8)
        del z,nrm,pos
    else: cover=np.ones((h,w),bool); deck=np.zeros((h,w),bool)
    rgb=a[...,:3].astype(np.float32); lum=luma(a)
    norm=rgb/np.maximum(rgb.max(-1,keepdims=True),1)
    # No filters cross a current hue boundary. Gold, stars and glow retain identity.
    hue_edge=np.zeros((h,w),bool)
    for c in range(3):
        hue_edge |= (ndi.maximum_filter(norm[...,c],size=3)-ndi.minimum_filter(norm[...,c],size=3))>.09
    gold=(rgb[...,0]>rgb[...,1]*1.13)&(rgb[...,1]>rgb[...,2]*1.20)&(lum>65)
    glow=mg[...,2]>12 if key in ('hull','dh') or key.startswith('shell') else np.zeros((h,w),bool)
    white=(lum>145)&(norm.min(-1)>.73)
    labels,num=ndi.label(white)
    counts=np.bincount(labels.ravel())
    # Small bright components are decorative stars, not the broad white hull paint.
    stars=white & (counts[labels]<max(30,int(h*w*.000025)))
    boundary=dilate(hue_edge|gold|stars,2)
    ao_l=Image.fromarray(ao[...,:3]).convert('L').resize((w,h),Image.Resampling.BILINEAR)
    ao_l=np.asarray(ao_l,dtype=np.float32)
    # Only deep crevices are protected. Fine deck wood is not a structural seam.
    crevice=(ao_l<62)&(ao_l<blur(ao_l,max(3,w/512))*.68)
    crevice=dilate(crevice,2)
    return cover,deck,boundary,glow,crevice,ao_l

def response(a, game, mask, tile):
    L=luma(a)+1
    G=Image.fromarray(game[...,:3]).convert('L')
    x=np.log(np.clip((np.asarray(G,dtype=np.float32)+1)/(np.asarray(G.filter(ImageFilter.GaussianBlur(24)),dtype=np.float32)+1),.72,1.22))
    y=np.log(L/(blur(L,24)+1e-3))
    slopes=[]; corrs=[]
    for yy in range(0,L.shape[0],tile):
        for xx in range(0,L.shape[1],tile):
            sl=np.s_[yy:yy+tile,xx:xx+tile]; m=mask[sl]
            if m.sum()<tile*tile*.4: continue
            u=x[sl][m]; v=y[sl][m]
            if np.std(u)<.018:continue
            corr=float(np.corrcoef(u,v)[0,1]); slope=float(np.cov(u,v,ddof=0)[0,1]/np.var(u))
            if np.isfinite(corr): corrs.append(corr); slopes.append(slope)
    return {'tiles':len(corrs),'correlation_median':float(np.median(corrs)) if corrs else None,
            'slope_median':float(np.median(slopes)) if slopes else None}

def contact(key, arrays, labels):
    width=1024; height=1024
    canvas=Image.new('RGB',(width*len(arrays),height+36),(24,25,35)); d=ImageDraw.Draw(canvas)
    for i,(a,label) in enumerate(zip(arrays,labels)):
        im=Image.fromarray(a[...,:3].astype('uint8')).resize((width,height),Image.Resampling.LANCZOS)
        canvas.paste(im,(i*width,36)); d.text((i*width+12,9),label,fill='white')
    canvas.save(OUT/'evidence'/f'{key}-contact.png')

def refine_shell(a,n,ao):
    """Remove the old atlas's procedural cloud and halve its broad fake recess.

    The current atlas matches prop_atlas.py within BC1 error (MAE 1.482/255).
    Reconstruct only its documented scalar fields, retaining current RGB, B/A,
    central seam, outer line and rivet positions. Identical shell variants share it.
    """
    size=a.shape[0]; cw=size//8
    vrow=1-np.arange(size)/size
    tt=np.clip((vrow-.62)/.36,0,1)[:,None]
    u=np.linspace(0,1,cw)[None,:]; uu=np.clip((u-.08)/.84,0,1)
    smooth=lambda x:x*x*(3-2*x)
    broad=.36+.64*smooth(np.clip(tt/.40,0,1))
    side=np.where(np.abs(uu-.5)>.40,.48,1.0)
    lift=((.5+.5*broad)/broad)*((.5+.5*side)/side)
    rng=np.random.default_rng(7)
    mot=np.kron(rng.normal(0,1,(size//32+1,cw//32+1)),np.ones((32,32)))[:size,:cw]
    mot=np.asarray(Image.fromarray((mot*40+128).clip(0,255).astype('uint8')).filter(ImageFilter.GaussianBlur(10)),float)/40-3.2
    noise=1+.03*mot
    f=broad*np.where(tt>.90,1.34,1.0)*np.where(tt<.08,.40,1.0)*side
    f=f*np.where(np.abs(tt-.5)<8/(.36*size),.32,1.0)
    edge=((np.abs(uu-.5)>.36)&(np.abs(uu-.5)<.44))|(((tt<.14)|(tt>.86))&(tt>.04)&(tt<.96))
    f=f*np.where(edge,1.10,1.0)
    py=tt*.36*size; px=uu*.84*cw
    for tr in (.13,.86):
        for k in range(10):
            d=np.sqrt((px-(.05+.1*k)*.84*cw)**2+(py-tr*.36*size)**2)
            f=f*np.where(d<2.8,.72,np.where(d<4.0,1.12,1.0))
    current_height=f*noise; refined_height=f*lift
    hnew=np.zeros((size,size),np.float32); top=vrow>=.61
    result=a.copy(); occ=ao.copy()
    for col in range(2,6):
        sl=slice(col*cw,(col+1)*cw)
        mult=lift/noise
        for im in [result,occ]:
            for reverse in [False,True]:
                target=im[::-1] if reverse else im
                rgb=target[top,sl,:3].astype(float)*mult[top,:,None]
                target[top,sl,:3]=np.clip(np.rint(rgb),0,255)
        hnew[top,sl]=refined_height[top]
        hnew[::-1][top,sl]=refined_height[top]
    hx=np.zeros_like(hnew);hy=np.zeros_like(hnew)
    hx[:,1:-1]=(hnew[:,2:]-hnew[:,:-2])*.5;hy[1:-1,:]=(hnew[2:,:]-hnew[:-2,:])*.5
    nx=-hx*4.5; ny=hy*4.5; inv=1/np.sqrt(nx*nx+ny*ny+1)
    calculated=np.stack([(nx*inv+1)*127.5,(ny*inv+1)*127.5],-1)
    normal=n.copy()
    panel=np.zeros((size,size),bool)
    panel[:,2*cw:6*cw]=((vrow>=.61)|(vrow<=.39))[:,None]
    normal[panel,:2]=np.clip(np.rint(calculated[panel]),0,255)
    return result,normal,occ

def restore_ring_relief(key,normal,original):
    # Independent visual audit localized actual hatch rings/spokes, in 1024 UV coordinates.
    boxes={'dh':[(9,572,115,681),(152,597,215,658)],
           'hull':[(139,818,182,858),(138,876,182,919)]}.get(key,[])
    changed=0
    for x0,y0,x1,y1 in boxes:
        x0,y0,x1,y1=[v*4 for v in (x0,y0,x1,y1)]
        h,w=y1-y0,x1-x0;yy,xx=np.mgrid[:h,:w]
        radius=np.sqrt(((xx-(w-1)/2)/(w/2))**2+((yy-(h-1)/2)/(h/2))**2)
        ring=np.clip((1.02-radius)*20,0,1)*np.clip((radius-.10)*15,0,1)
        old=original[y0:y1,x0:x1,:2].astype(float)-128
        now=normal[y0:y1,x0:x1,:2].astype(float)-128
        strength=np.linalg.norm(old,axis=-1)
        # Restore only actual source groove response; do not return flat dirt or an entire island.
        mask=(strength>6)&(np.linalg.norm(now,axis=-1)<strength*.42)
        weight=ring*mask
        restored=now+(old*.42-now)*weight[...,None]
        normal[y0:y1,x0:x1,:2]=np.clip(np.rint(restored+128),0,255)
        changed+=int((weight>0).sum())
    return changed

def uv_normal_guard(key):
    """Protect real UV discontinuities before a six-pixel-radius normal filter."""
    z=np.load(OUT/'inputs'/f'{key}_pos_4096.npz')
    pos=z['pos']; nrm=z['nrm']; cov=z['cov']>0
    edge=~cov
    for axis in (0,1):
        sl1=(slice(1,None),slice(None)) if axis==0 else (slice(None),slice(1,None))
        sl0=(slice(None,-1),slice(None)) if axis==0 else (slice(None),slice(None,-1))
        jump=(np.linalg.norm(pos[sl1]-pos[sl0],axis=-1)>.06)|(np.linalg.norm(nrm[sl1]-nrm[sl0],axis=-1)>.8)
        edge[sl1]|=jump;edge[sl0]|=jump
    return dilate(edge,7)

def clean_material(key,a,mg,cover,crevice):
    result=mg.copy()
    if key.startswith('shell'):return result,{'mode':'authored-uniform-R0-G109-retained'}
    rgb=a[...,:3].astype(float); lum=luma(a)
    gold=(rgb[...,0]>rgb[...,1]*1.13)&(rgb[...,1]>rgb[...,2]*1.20)&(lum>65)
    color_boundary=np.zeros(lum.shape,bool)
    # Preserve intentional metallic boundaries; smoothly feather into them.
    color_boundary|=dilate(gold,2)&~ndi.minimum_filter(gold,size=5)
    weight=soften(cover&~crevice&~color_boundary,4)
    # BC1 shares RGB endpoints. Keep every source block touching B emission
    # unchanged so the final compressed B coverage is byte-exact, not merely close.
    hh,ww=mg.shape[:2]; bh,bw=(hh+3)//4,(ww+3)//4
    emit=np.pad(mg[...,2]>0,((0,bh*4-hh),(0,bw*4-ww)))
    emit=emit.reshape(bh,4,bw,4).max((1,3)).repeat(4,0).repeat(4,1)[:hh,:ww]
    weight[emit]=0
    if key=='barrel':
        # Barrels remain metal; de-noise their material response rather than paint them.
        for c in (0,1):
            median=float(np.median(mg[...,c][cover&~crevice]))
            result[...,c]=np.clip(np.rint(mg[...,c]*(1-weight)+median*weight),0,255)
        return result,{'mode':'barrel-metal-median','target_RG':[float(np.median(mg[...,c][cover&~crevice])) for c in (0,1)]}
    # Preserve the existing artistic values. Current client evidence establishes
    # R=gloss/G=metal; these values originated under the old swapped preview
    # interpretation and must not be described as physically calibrated satin.
    # Weather cleanup is intentionally independent of any later MG tuning.
    navy=lum<88
    target_r=np.where(gold,185,8)
    target_g=np.where(gold,132,np.where(navy,58,96))
    for c,target in [(0,target_r),(1,target_g)]:
        result[...,c]=np.clip(np.rint(mg[...,c]*(1-weight)+target*weight),0,255)
    return result,{'mode':'current-color-coating-zones','paint_RG':[8,96],'navy_RG':[8,58],'gold_RG':[185,132],
                   'reference':'current authored shell R0/G109; original hull/DH R/G match game within BC1 error'}

def trim_secondary_splatter(a):
    """Visual-confirmed FGS053 muzzle-board ROI, keeping three dark aperture cores."""
    result=a.copy(); changed=np.zeros(a.shape[:2],bool)
    x0,y0,x1,y1=378,215,512,355
    crop=result[y0:y1,x0:x1,:3].astype(float)
    blue=(crop[...,2]>crop[...,0]*1.23)&(crop[...,2]>crop[...,1]*1.08)&(crop.max(-1)-crop.min(-1)>25)
    yy,xx=np.mgrid[-2:3,-2:3]; disk=xx*xx+yy*yy<=4
    core=ndi.binary_opening(blue,structure=disk)
    labels,_=ndi.label(core); counts=np.bincount(labels.ravel())
    core&=counts[labels]>=14
    removed=blue&~core
    white=(crop.min(-1)>145)&((crop.max(-1)-crop.min(-1))<65)
    if white.any():
        _,indices=ndi.distance_transform_edt(~white,return_indices=True)
        replacement=crop[indices[0],indices[1]]
        # One-pixel edge roll-off avoids pixel-step cutouts at 512 resolution.
        strength=np.where(ndi.distance_transform_edt(~core)>1.0,1.0,.65)*removed
        crop=crop*(1-strength[...,None])+replacement*strength[...,None]
        result[y0:y1,x0:x1,:3]=np.clip(np.rint(crop),0,255)
        changed[y0:y1,x0:x1]=removed
    return result,changed

def rounded_secondary_collar(a):
    """Rebuild the reviewed muzzle-board collar from its three actual aperture holes.

    This is an albedo-only post-refinement operation. The old 185-pixel speck
    cleanup was insufficient in the matched 3D view: connected peaks also need
    a controlled silhouette. Hole pixels, pale-violet markings and neutral dark
    structural lines are retained; removed peaks use local harmonic continuation.
    """
    result=a.copy();x0,y0,x1,y1=378,215,512,355
    original=result[y0:y1,x0:x1,:3].astype(float)
    blue=(original[...,2]>original[...,0]*1.23)&(original[...,2]>original[...,1]*1.08)&(original.max(-1)-original.min(-1)>25)
    holes=ndi.binary_fill_holes(blue)&~blue
    labels,count=ndi.label(holes);sizes=np.bincount(labels.ravel())
    ids=[i for i in range(1,count+1) if sizes[i]>=150]
    assert len(ids)==3,('Expected three source aperture holes',[(i,int(sizes[i])) for i in ids])
    centers=sorted([ndi.center_of_mass(holes,labels,i) for i in ids],key=lambda q:q[1])
    hole_mask=np.isin(labels,ids)
    hole_guard=ndi.binary_dilation(hole_mask,iterations=1)
    violet=(original[...,0]>110)&(original[...,0]-original[...,1]>5)&(original[...,2]-original[...,0]>15)
    neutral_seam=(original.max(-1)-original.min(-1)<12)&(original.mean(-1)<105)
    protect=hole_guard|violet|neutral_seam
    yy,xx=np.mgrid[:original.shape[0],:original.shape[1]]
    cy=float(np.mean([q[0] for q in centers]));left=float(centers[0][1]);right=float(centers[-1][1])
    radius=max(np.sqrt(sizes[i]/np.pi) for i in ids)+6.0
    dx=xx-np.clip(xx,left,right);distance=np.sqrt(dx*dx+(yy-cy)**2)
    coverage=np.clip(radius+.5-distance,0,1)
    # Clear the old irregular contour, including its one/two-pixel blue AA fringe.
    fill=ndi.binary_dilation(blue,iterations=2)&~protect
    index=np.full(fill.shape,-1,int);index[fill]=np.arange(int(fill.sum()))
    row=[];col=[];data=[];rhs=np.zeros((int(fill.sum()),3),float)
    for y,x in zip(*np.where(fill)):
        r=index[y,x];degree=0
        for dy,dx0 in ((-1,0),(1,0),(0,-1),(0,1)):
            ny,nx=y+dy,x+dx0
            if not (0<=ny<fill.shape[0] and 0<=nx<fill.shape[1]):continue
            degree+=1
            if fill[ny,nx]:row.append(r);col.append(index[ny,nx]);data.append(-1.0)
            else:rhs[r]+=original[ny,nx]
        row.append(r);col.append(r);data.append(float(degree))
    matrix=sparse.csr_matrix((data,(row,col)),shape=(len(rhs),len(rhs)))
    background=original.copy();background[fill]=spsolve(matrix,rhs)
    navy=np.median(original[blue&~protect&(coverage>.99)],axis=0)
    revised=background*(1-coverage[...,None])+navy*coverage[...,None]
    affected=fill|(coverage>0)
    revised[~affected|protect]=original[~affected|protect]
    result[y0:y1,x0:x1,:3]=np.clip(np.rint(revised),0,255)
    changed=np.any(result!=a,axis=-1)
    assert np.array_equal(result[y0:y1,x0:x1,:3][protect],a[y0:y1,x0:x1,:3][protect])
    full_protect=np.zeros(a.shape[:2],bool);full_protect[y0:y1,x0:x1]=protect
    stats={'roi':[x0,y0,x1,y1],'hole_centers_xy':[[q[1]+x0,q[0]+y0] for q in centers],
           'collar_radius_px':float(radius),'collar_center_y':cy+y0,'navy_rgb':navy.tolist(),
           'changed_pixels':int(changed.sum()),'protected_hole_pixels':int(hole_guard.sum()),
           'protected_violet_pixels':int(violet.sum()),'protected_neutral_seam_pixels':int(neutral_seam.sum()),
           'protected_pixels_exact_before_encoding':True,'alpha_exact_before_encoding':bool(np.array_equal(result[...,3],a[...,3]))}
    return result,changed,full_protect,stats

def secondary_collar_revision():
    """Targeted FGS053 albedo-only revision; never re-import normal/MG/AO."""
    key='fgs053';ch='a';name=NAMES[key]+'_a.dds'
    folder=OUT/'revisions/secondary-collar';folder.mkdir(parents=True,exist_ok=True)
    before_png=folder/'before.png';before_dds=folder/'before.dds'
    target=STAGE/source_path(key,ch).relative_to(SOURCE)
    if not before_png.exists():shutil.copy2(OUT/'edited/fgs053_a.png',before_png)
    if not before_dds.exists():shutil.copy2(target,before_dds)
    assets_before={str(p.relative_to(STAGE)):sha(p) for p in (STAGE/'assets').rglob('*') if p.is_file()}
    original=image(before_png)
    revised,changed,protected,stats=rounded_secondary_collar(original)
    save(folder/'after.png',revised);save(OUT/'edited/fgs053_a.png',revised)
    save(folder/'changed.png',changed*255);save(folder/'protected.png',protected*255)
    np.savez_compressed(folder/'masks.npz',changed=changed,protected=protected)
    contact('fgs053-collar',[original,revised],['PREVIOUS REFINEMENT','CONTROLLED ROUNDED COLLAR'])
    x0,y0,x1,y1=stats['roi']
    contact('fgs053-collar-close',[original[y0:y1,x0:x1],revised[y0:y1,x0:x1]],['PREVIOUS MUZZLE ROI','ROUNDED MUZZLE ROI'])
    cli('tex','import',OUT/'edited/fgs053_a.png','--like',name,'--project',STAGE,'--quality','high','--mip-filter','srgb')
    blocks=preserve_compressed_blocks(before_dds,target,changed)
    info=cli('tex','info',name,'--project',STAGE)['texture']
    mips=[]
    for mip in range(info['levels']):
        png=OUT/'verify'/f'fgs053_a_mip{mip}.png'
        cli('tex','export',name,'--project',STAGE,'--mip',mip,'-o',png)
        actual=image(png);expected=(max(1,info['width']>>mip),max(1,info['height']>>mip))
        assert Image.open(png).size==expected
        mips.append({'mip':mip,'size':list(expected),'alpha_opaque':bool(np.all(actual[...,3]==255))})
    for rel,digest in assets_before.items():
        if rel!=str(target.relative_to(STAGE)):assert sha(STAGE/rel)==digest,('Unrelated resource changed',rel)
    actual=image(OUT/'verify/fgs053_a_mip0.png')
    stats.update({'before_dds_sha256':sha(before_dds),'after_dds_sha256':sha(target),
                  'only_changed_resource':str(target.relative_to(STAGE)),
                  'other_resource_hashes_unchanged':True,'mips':mips,'block_preservation':blocks,
                  'protected_after_encode_mae_rgb':np.abs(actual[protected,:3].astype(float)-original[protected,:3]).mean(0).tolist()})
    dump(folder/'verification.json',stats)
    masks_path=OUT/'masks/fgs053.npz';prior=dict(np.load(masks_path));prior['changed_a']=np.any(revised!=raw(key,'a'),axis=-1);np.savez_compressed(masks_path,**prior)
    reports=json.loads((OUT/'dds-info.json').read_text(encoding='utf-8'));reports[name]['after']=info
    reports[name]['secondary_collar_revision']=stats;dump(OUT/'dds-info.json',reports)
    manifest=json.loads((OUT/'patch-manifest.json').read_text(encoding='utf-8'))
    for entry in manifest['files']:
        if entry['path']==str(target.relative_to(STAGE)).replace('\\','/'):
            entry['refined_sha256']=sha(target);entry['size']=target.stat().st_size
    dump(OUT/'patch-manifest.json',manifest)
    verification_path=OUT/'verification.json'
    verification=json.loads(verification_path.read_text(encoding='utf-8'))
    error=np.abs(actual.astype(np.int16)-revised.astype(np.int16))
    untouched=~prior['changed_a'];source_image=raw(key,'a')
    verification['pixel_checks'][key]['a']={
        'compression_mae_rgba':error.mean((0,1)).tolist(),
        'compression_p99_rgba':np.percentile(error,99,axis=(0,1)).tolist(),
        'unchanged_region_mae_rgba':np.abs(actual[untouched].astype(float)-source_image[untouched]).mean(0).tolist(),
    }
    verification['targeted_secondary_collar_revision']=stats
    verification['packed_channel_checks_retained']='All other resource hashes unchanged; prior 161 n/mg level checks still apply.'
    dump(verification_path,verification)
    statistics_path=OUT/'evidence/fgs053-stats.json'
    statistics=json.loads(statistics_path.read_text(encoding='utf-8'))
    statistics['collar_revision']=stats
    statistics['color_changed']=int(prior['changed_a'].sum())
    statistics['protection_exception']='Matched 3D review authorized rounded three-hole collar and peak removal; hole/violet pixels protected.'
    dump(statistics_path,statistics)
    print(json.dumps(stats),flush=True)

def process(key):
    a=raw(key,'a'); n=raw(key,'n'); mg=raw(key,'mg'); ao=raw(key,'ao')
    cover,deck,boundary,glow,crevice,ao_l=regions(key,a,mg,ao)
    eligible=cover&~boundary&~dilate(glow,2)&~crevice
    weight=soften(eligible,3)
    edited=a.copy(); stats={}
    if not key.startswith('shell'):
        game=raw(key,'a',True)
        gl=Image.fromarray(game[...,:3]).convert('L')
        factor=np.clip((np.asarray(gl,dtype=np.float32)+1)/(np.asarray(gl.filter(ImageFilter.GaussianBlur(24)),dtype=np.float32)+1),.72,1.22)
        exponent={'hull':.51,'dh':.54,'fgm':.49,'fgs036':.51,'fgs053':.52,'barrel':.70}[key]
        # Reverse the legacy multiplication without borrowing pixels across designs.
        rgb=a[...,:3].astype(np.float32)*factor[...,None]**(-exponent*weight[...,None])
        edited[...,:3]=np.clip(np.rint(rgb),0,255)
        stats['legacy_factor_exponent']=exponent
        stats['response_before']=response(a,game,eligible,128 if a.shape[0]>1024 else 64)
        stats['response_after']=response(edited,game,eligible,128 if a.shape[0]>1024 else 64)
    else:
        edited,_,_=refine_shell(a,n,ao)
        stats['shell_broad_fake_recess_reduction']=.50
        stats['shell_cloud_removed']='deterministic seed-7 scalar inversion; current DDS remains RGB base'
    # Glow follows the current B mask only; retain local brightness and coverage.
    if glow.any():
        rgb=edited[...,:3].astype(np.float32); lum=luma(edited)
        cyan=glow&(rgb[...,2]>rgb[...,0]*1.45)&(rgb[...,1]>rgb[...,0]*1.30)
        target=np.array([40,137,212],np.float32)
        target_l=float(.299*target[0]+.587*target[1]+.114*target[2])
        tint=target[None,None,:]*np.clip(lum/target_l,.55,1.35)[...,None]
        amount=(mg[...,2].astype(np.float32)/255)*cyan*.45
        rgb=rgb*(1-amount[...,None])+tint*amount[...,None]
        edited[...,:3]=np.clip(np.rint(rgb),0,255)
        stats['glow_tinted_pixels']=int(cyan.sum())
        stats['glow_mask_unchanged']=True
    splatter=np.zeros(a.shape[:2],bool)
    if key=='fgs053':
        edited,splatter=trim_secondary_splatter(edited)
        stats['visual_confirmed_muzzle_splatter_pixels']=int(splatter.sum())
    # Normal RGB channels have separate source semantics: edit R/G exclusively.
    normal=n.copy()
    normal_weight=soften(cover&~boundary&~dilate(glow,2)&~crevice,4)
    if key in ('hull','dh'):
        normal_weight[uv_normal_guard(key)]=0
        stats['normal_filter_uv_guard_radius']=7
    if key.startswith('shell'):
        normal_weight[:]=0
        _,normal,ao_edited=refine_shell(a,n,ao)
    else:
        ao_edited=ao.copy()
        sigma=3 if key in ('hull','dh') else max(.65,a.shape[0]/512)
        for c in (0,1):
            v=n[...,c].astype(np.float32)-128
            low=ndi.gaussian_filter(v,sigma,mode='nearest',truncate=2 if key in ('hull','dh') else 4)
            # Flat coated deck discards plank relief. Vertical panels retain 26% of broad form.
            target=low*np.where(deck,.025,.26)
            if key=='barrel': target=low*.40
            normal[...,c]=np.clip(np.rint(128+v+(target-v)*normal_weight),0,255)
    stats['localized_ring_relief_pixels']=restore_ring_relief(key,normal,n)
    normal[splatter,:2]=128
    material,material_policy=clean_material(key,edited,mg,cover,crevice)
    assert np.array_equal(material[...,2:],mg[...,2:])
    assert np.array_equal(normal[...,2:],n[...,2:]), 'B/A semantic channels must be exact before compression'
    changed_a=np.any(edited!=a,axis=-1); changed_n=np.any(normal!=n,axis=-1)
    changed_mg=np.any(material!=mg,axis=-1); changed_ao=np.any(ao_edited!=ao,axis=-1)
    stats.update({'pixels':int(cover.size),'uv_coverage':float(cover.mean()),'deck_coverage':float(deck.mean()),
        'color_changed':int(changed_a.sum()),'normal_changed':int(changed_n.sum()),
        'protected_color_pixels':int((boundary&~glow).sum()),
        'protected_color_exact':bool(np.array_equal(edited[boundary&~glow],a[boundary&~glow])),
        'normal_BA_exact_before_compression':True,'ao_unchanged':not bool(changed_ao.any()),'mg_unchanged':not bool(changed_mg.any()),
        'material_policy':material_policy,'mg_BA_exact_before_compression':True})
    if key=='fgs053':stats['protection_exception']='185-pixel visual-confirmed muzzle splatter repair; deep aperture cores retained'
    if key.startswith('shell'):stats['protection_exception']='authored broad panel illumination reduced; seam and rivet locations retained'
    flatmask=normal_weight>.95
    for label,im in [('before',n),('after',normal)]:
        mag=np.linalg.norm(im[...,:2].astype(np.float32)-128,axis=-1)
        stats['normal_'+label+'_RG_p90']=float(np.percentile(mag[flatmask],90)) if flatmask.any() else 0
    for channel,im in [('a',edited),('n',normal),('mg',material),('ao',ao_edited)]:
        save(OUT/'edited'/f'{key}_{channel}.png',im)
    np.savez_compressed(OUT/'masks'/f'{key}.npz',eligible=eligible,boundary=boundary,glow=glow,deck=deck,changed_a=changed_a,changed_n=changed_n,changed_mg=changed_mg,changed_ao=changed_ao)
    save(OUT/'evidence'/f'{key}-mask.png',np.stack([boundary*255,eligible*255,deck*160],-1))
    contact(key,[a,edited,np.clip((edited[...,:3].astype(float)-a[...,:3])*4+128,0,255)],['CURRENT','REFINED CANDIDATE','DIFFERENCE x4 +128'])
    contact(key+'-normal',[n,normal],['CURRENT NORMAL','REFINED NORMAL (B/A PRESERVED)'])
    contact(key+'-material',[mg,material],['CURRENT M/G/B','REFINED M/G/B (B MASK PRESERVED)'])
    if changed_ao.any():contact(key+'-ao',[ao,ao_edited],['CURRENT AO','REFINED AO (STRUCTURAL LINES RETAINED)'])
    dump(OUT/'evidence'/f'{key}-stats.json',stats)
    print(json.dumps({key:stats}),flush=True)

def preserve_compressed_blocks(source, output, changed):
    """Copy untouched source BC blocks verbatim at every mip, including split DDS.

    This is an exact compressed-block operation, not an image re-encoding pass.
    A conservative two-pixel footprint is expanded at each downsampled level.
    Game pointer / BC7Prep inputs have no directly transplantable local blocks.
    """
    if not source.exists() or source.read_bytes()[:4]!=b'DDS ': return {'status':'game-pointer-no-local-blocks'}
    masks=[changed]
    while max(masks[-1].shape)>1:
        m=ndi.maximum_filter(masks[-1],size=5)
        hh,ww=m.shape
        padded=np.pad(m,((0,hh%2),(0,ww%2)),mode='edge')
        masks.append(padded.reshape(padded.shape[0]//2,2,padded.shape[1]//2,2).max((1,3)))
    files=[source]+sorted(source.parent.glob(source.stem+'.dd[0-9]'))
    result=[]
    for src in files:
        dst=output.with_suffix(src.suffix)
        if not dst.exists(): continue
        old=src.read_bytes(); new=bytearray(dst.read_bytes())
        if old[:4]!=b'DDS ' or new[:4]!=b'DDS ': continue
        height,width=struct.unpack_from('<II',old,12)
        new_height,new_width=struct.unpack_from('<II',new,12)
        assert (height,width)==(new_height,new_width)
        fourcc=old[84:88]; header=148 if fourcc==b'DX10' else 128
        block=8 if fourcc==b'DXT1' else 16
        assert new[84:88]==fourcc and len(new)==len(old),(src,len(new),len(old))
        levels=max(1,struct.unpack_from('<I',old,28)[0])
        first=next(i for i,m in enumerate(masks) if m.shape==(height,width))
        offset=header; copied=0; total=0
        for mip in range(levels):
            m=masks[min(first+mip,len(masks)-1)]
            mh,mw=m.shape; bh,bw=max(1,(mh+3)//4),max(1,(mw+3)//4)
            pm=np.pad(m,((0,bh*4-mh),(0,bw*4-mw)))
            dirty=pm.reshape(bh,4,bw,4).max((1,3)).ravel()
            count=bh*bw; size=count*block
            original=np.frombuffer(old,np.uint8,count=size,offset=offset).reshape(count,block)
            current=np.frombuffer(new,np.uint8,count=size,offset=offset).reshape(count,block)
            current[~dirty]=original[~dirty]
            copied+=int((~dirty).sum()); total+=count; offset+=size
        assert offset==len(new),(src,offset,len(new))
        dst.write_bytes(new)
        result.append({'file':dst.name,'unchanged_blocks_copied':copied,'total_blocks':total})
    return {'status':'exact-blocks-restored','files':result}

def import_images(keys):
    report_path=OUT/'dds-info.json'
    reports=json.loads(report_path.read_text()) if report_path.exists() else {}
    for key in keys:
        masks=np.load(OUT/'masks'/f'{key}.npz')
        for ch in ['a','n','mg','ao']:
            if not masks['changed_'+ch].any(): continue
            name=NAMES[key]+f'_{ch}.dds'
            print('Encoding '+name,flush=True)
            before=cli('tex','info',name,'--project',SOURCE)['texture']
            cli('tex','import',OUT/'edited'/f'{key}_{ch}.png','--like',name,'--project',STAGE,'--quality','high','--mip-filter','srgb' if ch=='a' else 'linear')
            src=source_path(key,ch); dst=STAGE/src.relative_to(SOURCE)
            blocks=preserve_compressed_blocks(src,dst,masks['changed_'+ch])
            after=cli('tex','info',name,'--project',STAGE)['texture']
            fields=['width','height','format','dxgi','levels','volumes','tail','volume_files']
            assert {f:before[f] for f in fields}=={f:after[f] for f in fields}, name
            reports[name]={'before':before,'after':after,'block_preservation':blocks}
            for mip in range(min(4,after['levels'])):
                cli('tex','export',name,'--project',STAGE,'--mip',mip,'-o',OUT/'verify'/f'{key}_{ch}_mip{mip}.png')
            # No-op high-quality encode is a baseline for unavoidable quantization.
            if ch in ('n','mg'):
                noop=OUT/'noop'/name
                baseline=OUT/'verify'/f'{key}_{ch}_noop.png'
                if not baseline.exists():
                    cli('tex','import',OUT/'inputs'/f'{key}_{ch}.png','--like',name,'--project',STAGE,'--quality','high','--mip-filter','linear','-o',noop)
                    cli('tex','export',noop,'-o',baseline)
    dump(OUT/'dds-info.json',reports)
    material_path=OUT/'material-edits.json'
    material_edits=json.loads(material_path.read_text()) if material_path.exists() else []
    prefixes=tuple(NAMES.values())
    for p in (STAGE/'assets').rglob('*.mfm'):
        if not p.name.startswith(prefixes): continue
        old=p.read_text(encoding='utf-8'); new=old
        for prop in ('g_detailAlbedoInfluence','g_detailGlossInfluence','g_detailNormalInfluence'):
            new=re.sub(r'(<property>\s*'+prop+r'\s*<Float>\s*)[\d.]+',r'\g<1>0.0000',new)
        if new!=old:
            p.write_text(new,encoding='utf-8',newline='')
            relative=str(p.relative_to(STAGE))
            if relative not in material_edits:material_edits.append(relative)
    dump(material_path,material_edits)

def guard_normal_semantics(keys):
    """Reject rare BC endpoint outliers by retaining original compressed blocks."""
    path=OUT/'dds-info.json'; reports=json.loads(path.read_text(encoding='utf-8'))
    for key in keys:
        src=source_path(key,'n')
        if src.read_bytes()[:4]!=b'DDS ':continue
        actual_path=OUT/'verify'/f'{key}_n_mip0.png'
        if not actual_path.exists():continue
        original=raw(key,'n'); actual=image(actual_path)
        delta=np.abs(original[...,2:].astype(np.int16)-actual[...,2:].astype(np.int16))
        reject=(delta[...,0]>8)|(delta[...,1]>3)
        if not reject.any():continue
        h,w=reject.shape;bh,bw=(h+3)//4,(w+3)//4
        padded=np.pad(reject,((0,bh*4-h),(0,bw*4-w)))
        blocks=padded.reshape(bh,4,bw,4).max((1,3))
        reject_block=blocks.repeat(4,0).repeat(4,1)[:h,:w]
        masks=np.load(OUT/'masks'/f'{key}.npz')
        allowed=masks['changed_n']&~reject_block
        dst=STAGE/src.relative_to(SOURCE)
        preserve_compressed_blocks(src,dst,allowed)
        name=NAMES[key]+'_n.dds'
        reports[name]['compression_semantic_guard']={'B_limit':8,'A_limit':3,'source_blocks_retained':int(blocks.sum()),'original_outlier_pixels':int(reject.sum()),'total_pixels':int(h*w)}
        reports[name]['after']=cli('tex','info',name,'--project',STAGE)['texture']
        for mip in range(min(4,reports[name]['after']['levels'])):
            cli('tex','export',name,'--project',STAGE,'--mip',mip,'-o',OUT/'verify'/f'{key}_n_mip{mip}.png')
        print(key+' retained '+str(int(blocks.sum()))+' source normal blocks for B/A fidelity',flush=True)
    dump(path,reports)

def original_dds(key,ch):
    src=source_path(key,ch)
    if src.read_bytes()[:4]==b'DDS ':return src
    # Rewrite only the game asset to evidence: it decodes BC7Prep without
    # re-encoding blocks or rebuilding any mip. No project is passed or mutated.
    target=OUT/'original-blocks'/(NAMES[key]+f'_{ch}.dds')
    target.parent.mkdir(parents=True,exist_ok=True)
    if not target.exists():cli('tex','rewrite',target.name,'--from-game','-o',target)
    return target

def dds_levels(base):
    files=[base]+sorted(base.parent.glob(base.stem+'.dd[0-9]'))
    fullw=max(struct.unpack_from('<I',p.read_bytes(),16)[0] for p in files)
    levels={}
    for path in files:
        data=path.read_bytes(); assert data[:4]==b'DDS '
        h,w=struct.unpack_from('<II',data,12);fourcc=data[84:88]
        header_size=148 if fourcc==b'DX10' else 128
        block=8 if fourcc==b'DXT1' else 16
        count=max(1,struct.unpack_from('<I',data,28)[0]);offset=header_size
        first=int(round(np.log2(fullw/w)))
        for local in range(count):
            bw,bh=max(1,(w+3)//4),max(1,(h+3)//4);size=bw*bh*block
            header=bytearray(data[:header_size]);struct.pack_into('<II',header,12,h,w);struct.pack_into('<I',header,28,1)
            payload=data[offset:offset+size];assert len(payload)==size
            levels[first+local]={'path':path,'offset':offset,'size':size,'width':w,'height':h,'block_size':block,'header':bytes(header),'payload':payload}
            offset+=size;w=max(1,w//2);h=max(1,h//2)
        assert offset==len(data),(path,offset,len(data))
    return levels

def decode_level(level):
    return np.asarray(Image.open(io.BytesIO(level['header']+level['payload'])).convert('RGBA')).copy()

def replace_level(level,payload):
    assert len(payload)==level['size']
    data=bytearray(level['path'].read_bytes());off=level['offset']
    data[off:off+len(payload)]=payload;level['path'].write_bytes(data);level['payload']=bytes(payload)

@lru_cache(maxsize=None)
def neutral_bc7_endpoint(bits,pbit):
    precision=bits+(pbit is not None);best=None
    for encoded in range(1<<bits):
        q=(encoded<<1)|pbit if pbit is not None else encoded
        expanded=q<<(8-precision);expanded|=expanded>>precision
        rank=(abs(expanded-136),encoded)
        if best is None or rank<best[0]:best=(rank,encoded)
    return best[1]

def neutral_bc7_blue(block):
    """Change only the decoded B component's endpoint fields.

    Layout source: Khronos DataFormat bptc.txt, tables 20.1/20.3. Partition,
    rotation, endpoint P-bits, index selector and all indices remain unchanged.
    Fixed source P-bits can quantize 136 to 132..140; that entire interval has
    verified zero local weather gate and detail layer 8 in the actual shaders.
    """
    value=int.from_bytes(block,'little');low=value&255
    assert low, 'Reserved BC7 mode cannot be safely patched'
    mode=(low&-low).bit_length()-1
    # subsets, partition, rotation, index selector, RGB bits, A bits, endpoint P, shared P
    modes=[(3,4,0,0,4,0,1,0),(2,6,0,0,6,0,0,1),(3,6,0,0,5,0,0,0),
           (2,6,0,0,7,0,1,0),(1,0,2,1,5,6,0,0),(1,0,2,0,7,8,0,0),
           (1,0,0,0,7,7,1,0),(2,6,0,0,5,5,1,0)]
    subsets,pb,rb,isb,cb,ab,epb,spb=modes[mode];endpoints=2*subsets
    cursor=mode+1+pb;rotation=(value>>cursor)&((1<<rb)-1) if rb else 0
    start=cursor+rb+isb;p_start=start+endpoints*(3*cb+ab)
    alpha_source=rotation==3
    bits=ab if alpha_source else cb
    field_start=start+endpoints*(3*cb if alpha_source else 2*cb)
    for endpoint in range(endpoints):
        pbit=None
        if epb:pbit=(value>>(p_start+endpoint))&1
        elif spb:pbit=(value>>(p_start+endpoint//2))&1
        shift=field_start+endpoint*bits;mask=((1<<bits)-1)<<shift
        value=(value&~mask)|(neutral_bc7_endpoint(bits,pbit)<<shift)
    return value.to_bytes(16,'little'),mode

def weather_gate(blue):
    return np.clip(np.abs(blue.astype(np.float64)/255-.533333)*31.874998-1,0,1)**2

def weather_probe():
    """Isolated B-only port A/B; never writes final project, game or texture-stage."""
    final=Path('E:/WoWSModding/out/robin-refine-20261116/project')
    probe=Path('E:/WoWSModding/out/robin-refine-20261116/weather-stage')
    folder=OUT/'weather-probe';folder.mkdir(parents=True,exist_ok=True)
    baseline={str(p.relative_to(final)).replace('\\','/'):{'sha256':sha(p),'size':p.stat().st_size}
              for p in (final/'assets').rglob('*') if p.is_file()}
    if not probe.exists():shutil.copytree(final,probe,ignore=shutil.ignore_patterns('.wowsmod','.install','edits'))
    else:
        prior=folder/'baseline.json'
        assert prior.exists() and json.loads(prior.read_text())==baseline,'Existing probe has a different base'
    dump(folder/'baseline.json',baseline)
    checks={};changes=[];binding_checks={}
    resroot=final/'assets/res_mods'
    for visual in (resroot/'PnFMods/Robin_Bourgogne/FSB510_Bourgogne').rglob('*.visual'):
        refs=re.findall(r'<mfm>\s*(.*?)\s*</mfm>',visual.read_text(encoding='utf-8'))
        relevant=[r for r in refs if any(q in r for q in ('FSB025_Bourgogne_1945_Hull','FGM049_380mm_45_Mle_1935'))]
        if not relevant:continue
        entries=[]
        for ref in sorted(set(relevant)):
            material=resroot/ref;assert material.exists(),ref
            txt=material.read_text(encoding='utf-8')
            props={m[0].strip():m[2].strip() for m in re.findall(r'<property>\s*([^<]+)<(Texture|Float)>\s*(.*?)\s*</\2>\s*</property>',txt,re.S)}
            entries.append({'mfm':ref,'shader':re.search(r'<fx>\s*(.*?)\s*</fx>',txt)[1],
                            'properties':props,'sha256':sha(material)})
        binding_checks[str(visual.relative_to(resroot))]=entries
    dump(folder/'material-bindings.json',binding_checks)
    for key in ('fgm','hull'):
        rel=source_path(key,'n').relative_to(SOURCE);src=final/rel;dst=probe/rel
        before=dds_levels(src)
        for entry in before.values():
            rollback=folder/'rollback'/entry['path'].relative_to(final)
            rollback.parent.mkdir(parents=True,exist_ok=True)
            if not rollback.exists():shutil.copy2(entry['path'],rollback)
            shutil.copy2(entry['path'],probe/entry['path'].relative_to(final))
        current=dds_levels(dst)
        a=decode_level(dds_levels(final/source_path(key,'a').relative_to(SOURCE))[0])
        mg=decode_level(dds_levels(final/source_path(key,'mg').relative_to(SOURCE))[0])
        ao=decode_level(dds_levels(final/source_path(key,'ao').relative_to(SOURCE))[0]) if key=='hull' else raw(key,'ao')
        lum=luma(a)
        if key=='fgm':
            z=np.load(OUT/'port-diagnosis/fgm-pos.npz')
            mask=(z['cov']>0)&(z['nrm'][...,1]>.45)&(z['pos'][...,1]>.16)&(a[...,:3].min(-1)>100)&(lum>135)
            rule='current original FGM mesh: covered, local normal.y>.45, local y>.16, RGB min>100, L>135'
        else:
            z=np.load(OUT/'inputs/hull_pos_4096.npz')
            up=(z['upc']>0)|(z['wrapc']>0)|(z['nrm'][...,1]>.65)
            ao_l=np.asarray(Image.fromarray(ao).convert('L').resize((a.shape[1],a.shape[0]),Image.Resampling.BILINEAR))
            mask=(z['cov']>0)&up&(z['pos'][...,1]>.05)&(z['pos'][...,1]<.65)&(lum>70)&(ao_l>64)
            rule='current Hull covered upward/repeated UV, world .05<y<.65, L>70, AO>64'
        mask=ndi.binary_erosion(mask,iterations=2)
        save(folder/f'{key}-semantic-region.png',mask*255)
        mask_mip=mask.copy();level_checks=[]
        for mip in sorted(before):
            old=decode_level(before[mip]);level=current[mip]
            h,w=old.shape[:2];bh,bw=max(1,(h+3)//4),max(1,(w+3)//4)
            assert mask_mip.shape==(h,w)
            padded=np.pad(mask_mip,((0,bh*4-h),(0,bw*4-w)),constant_values=False)
            selected=padded.reshape(bh,4,bw,4).all((1,3)).ravel()
            original_payload=np.frombuffer(level['payload'],np.uint8).reshape(-1,16)
            payload=original_payload.copy();modes={}
            for block_index in np.flatnonzero(selected):
                revised,mode=neutral_bc7_blue(payload[block_index].tobytes())
                payload[block_index]=np.frombuffer(revised,np.uint8);modes[str(mode)]=modes.get(str(mode),0)+1
            replace_level(level,payload.tobytes());new=decode_level(level)
            pixelmask=selected.reshape(bh,bw).repeat(4,0).repeat(4,1)[:h,:w]
            assert np.array_equal(old[...,[0,1,3]],new[...,[0,1,3]]),(key,mip,'R/G/A changed')
            assert np.array_equal(old[~pixelmask],new[~pixelmask]),(key,mip,'control changed')
            assert np.array_equal(original_payload[~selected],payload[~selected]),'Control block bytes changed'
            w_before=weather_gate(old[...,2][pixelmask]);w_after=weather_gate(new[...,2][pixelmask])
            if pixelmask.any():
                assert float(w_after.max())<1e-10,(key,mip,'weather gate not neutral')
                detail=np.minimum(np.floor(15*new[...,2][pixelmask].astype(float)/255+.5),15)
                assert np.all(detail==8),(key,mip,'detail selector drift')
            item={'mip':mip,'size':[w,h],'selected_blocks':int(selected.sum()),'total_blocks':int(selected.size),
                  'selected_pixels':int(pixelmask.sum()),'RGA_exact':True,'outside_pixels_and_blocks_exact':True,'modes':modes,
                  'old_B_quantiles':np.percentile(old[...,2][pixelmask],[0,10,50,90,100]).tolist() if pixelmask.any() else [],
                  'new_B_range':[int(new[...,2][pixelmask].min()),int(new[...,2][pixelmask].max())] if pixelmask.any() else [],
                  'W_before_mean':float(w_before.mean()) if pixelmask.any() else None,
                  'W_before_positive_fraction':float((w_before>0).mean()) if pixelmask.any() else None,
                  'W_after_max':float(w_after.max()) if pixelmask.any() else None}
            level_checks.append(item)
            save(folder/f'{key}-mask-mip{mip}.png',pixelmask*255)
            if mip==0:
                selected_mg=mg[pixelmask,:3]
                pairs,counts=np.unique(selected_mg[:,:2],axis=0,return_counts=True)
                item['MG_top_RG_pairs']=[{'RG':pairs[i].tolist(),'fraction':float(counts[i]/len(selected_mg))} for i in np.argsort(counts)[-5:][::-1]]
                item['MG_B_nonzero_fraction']=float((selected_mg[:,2]>0).mean())
                overlay=a.copy();overlay[pixelmask,:3]=(overlay[pixelmask,:3]*.4+np.array([20,220,90])*.6).astype(np.uint8)
                save(folder/f'{key}-mask-overlay.png',overlay)
                contact(key+'-weather-probe',[np.repeat(old[...,2:3],3,-1),np.repeat(new[...,2:3],3,-1),overlay],['CURRENT NORMAL B','B-ONLY LOCAL GATE PROBE','GREEN = TARGETED COATING'])
            if max(h,w)>1:
                padded_mask=np.pad(mask_mip,((0,h%2),(0,w%2)),constant_values=False)
                mask_mip=padded_mask.reshape(padded_mask.shape[0]//2,2,padded_mask.shape[1]//2,2).all((1,3))
        checks[key]={'selection':rule,'source_normal':str(src),'levels':level_checks}
    for path in (probe/'assets').rglob('*'):
        if not path.is_file():continue
        rel=str(path.relative_to(probe)).replace('\\','/');info=baseline[rel];digest=sha(path)
        if digest!=info['sha256']:
            assert path.stem in (NAMES['fgm']+'_n',NAMES['hull']+'_n') and path.suffix in ('.dds','.dd0','.dd1','.dd2'),rel
            changes.append({'path':rel,'original_sha256':info['sha256'],'refined_sha256':digest,'size':path.stat().st_size})
    for rel,info in baseline.items():assert sha(final/rel)==info['sha256'],'Final project changed during probe preparation'
    dump(folder/'verification.json',{'source_final_project_unchanged':True,'source_project':str(final),'probe_project':str(probe),
         'scope':'only B endpoints on complete blocks in original FGM coated roof and low Hull coated deck',
         'weather_gate':'saturate(abs(B/255-.533333)*31.874998-1)^2','does_not_disable_all_weather':True,
         'all_other_assets_identical':True,'checks':checks,'files_changed':len(changes)})
    dump(folder/'patch-manifest.json',{'source':str(final),'stage':str(probe),'rollback':str(folder/'rollback'),'files':changes})
    print(json.dumps({'probe':str(probe),'files_changed':len(changes),'verification':str(folder/'verification.json')}),flush=True)

def validate_weather_stage(current_hashes,baseline,previous_entries=()):
    """Read-only preflight: reject any unknown content before rollback writes."""
    expected={p:v['sha256'] for p,v in baseline.items()}
    for e in previous_entries:
        assert e['path'] in expected
        assert e['original_sha256']==baseline[e['path']]['sha256']
        expected[e['path']]=e['refined_sha256']
    assert set(current_hashes)==set(expected),'Weather stage file set changed'
    conflicts=[p for p,v in current_hashes.items() if v['sha256']!=expected[p]]
    assert not conflicts,('Refuse to overwrite later stage changes',conflicts)

def weather_final():
    """Production B-only coating cleanup after positive real-port A/B evidence.

    Preserve resolved mechanical openings. Authored FGM navy covers receive the
    same dry-coating B semantic, with their R/G folds and A preserved. At coarse
    levels, only coating-dominant mixtures with no resolved protected texel may
    use the dry-coating semantic. Exact R/G/A and exterior-block checks remain.
    """
    keys=('hull','dh','fgm','fgs036','fgs053')
    folder=OUT/'weather-final';folder.mkdir(parents=True,exist_ok=True)
    baseline_path=folder/'baseline-assets.json'
    current_hashes={str(p.relative_to(STAGE)).replace('\\','/'):{'sha256':sha(p),'size':p.stat().st_size}
                    for p in (STAGE/'assets').rglob('*') if p.is_file()}
    if not baseline_path.exists():dump(baseline_path,current_hashes)
    baseline=json.loads(baseline_path.read_text(encoding='utf-8'))
    # Verify the entire frozen stage before restoring even one rollback file.
    # A repeat run accepts only our previous complete output, never a later edit.
    previous_manifest=folder/'patch-manifest.json'
    previous_entries=json.loads(previous_manifest.read_text(encoding='utf-8'))['files'] if previous_manifest.exists() else []
    validate_weather_stage(current_hashes,baseline,previous_entries)
    materials={};root=STAGE/'assets/res_mods';target_names={NAMES[k]+'_n.dds' for k in keys}
    for mfm in (root/'PnFMods/Robin_Bourgogne/FSB510_Bourgogne').rglob('*.mfm'):
        txt=mfm.read_text(encoding='utf-8')
        props={x[0].strip():x[2].strip() for x in re.findall(r'<property>\s*([^<]+)<(Texture|Float)>\s*(.*?)\s*</\2>\s*</property>',txt,re.S)}
        normal=props.get('normalMap','')
        if Path(normal).name not in target_names:continue
        assert normal.startswith('PnFMods/Robin_Bourgogne/FSB510_Bourgogne/'),('External normal binding',mfm,normal)
        assert (root/normal).exists(),normal
        influences={p:float(props[p]) for p in ('g_detailAlbedoInfluence','g_detailGlossInfluence','g_detailNormalInfluence')}
        assert all(v==0 for v in influences.values()),('Detail index 8 unsafe on active detail material',mfm,influences)
        materials[str(mfm.relative_to(root)).replace('\\','/')]={'normal':normal,'detail_influences':influences,'sha256':sha(mfm)}
    assert {Path(v['normal']).name for v in materials.values()}==target_names
    visual_bindings={}
    for visual in (root/'PnFMods/Robin_Bourgogne/FSB510_Bourgogne').rglob('*.visual'):
        txt=visual.read_text(encoding='utf-8')
        refs=[v.strip() for v in re.findall(r'<mfm>(.*?)</mfm>',txt,re.S)]
        selected=[v for v in refs if v in materials]
        if selected:
            overrides=re.findall(r'<property>\s*g_detail(?:Albedo|Gloss|Normal)Influence\s*<Float>\s*([\d.]+)',txt)
            assert all(float(x)==0 for x in overrides),('Nonzero visual detail override',visual)
            visual_bindings[str(visual.relative_to(root)).replace('\\','/')]=sorted(set(selected))
    dump(folder/'material-bindings.json',{'materials':materials,'visuals':visual_bindings})
    checks={};incremental=[]
    for key in keys:
        print('Final weather semantics: '+key,flush=True)
        rel=source_path(key,'n').relative_to(SOURCE);dst=STAGE/rel
        rollback=folder/'rollback'/rel;rollback.parent.mkdir(parents=True,exist_ok=True)
        source_files=[dst]+sorted(dst.parent.glob(dst.stem+'.dd[0-9]'))
        for p in source_files:
            backup=folder/'rollback'/p.relative_to(STAGE);backup.parent.mkdir(parents=True,exist_ok=True)
            if not backup.exists():shutil.copy2(p,backup)
            assert sha(backup)==baseline[str(p.relative_to(STAGE)).replace('\\','/')]['sha256']
            shutil.copy2(backup,p)
        original_levels=dds_levels(rollback);levels=dds_levels(dst)
        a=decode_level(dds_levels(STAGE/source_path(key,'a').relative_to(SOURCE))[0])
        game=raw(key,'a',True);ao=raw(key,'ao');lum=luma(a);game_lum=luma(game)
        ao_l=np.asarray(Image.fromarray(ao).convert('L').resize((a.shape[1],a.shape[0]),Image.Resampling.BILINEAR))
        positions=OUT/('inputs/'+key+'_pos_4096.npz' if key in ('hull','dh') else 'port-diagnosis/'+key+'-pos.npz')
        uv=np.load(positions);cov=uv['cov']>0
        hard=cov&((lum<34)|((ao_l<28)&(game_lum<38)&(lum<100)))
        authored_fabric=cov&(lum<100)&(a[...,2].astype(float)>a[...,0].astype(float)*1.3) if key=='fgm' else np.zeros(cov.shape,bool)
        # Explicit post-port art decision: the recolored navy fabric is also a
        # clean theme surface. B has weather/detail meanings; detail is disabled.
        # Its folds are encoded in R/G and remain bit-exact at every mip.
        fabric=np.zeros(cov.shape,bool)
        coating=cov&~hard&~fabric
        padding=dilate(coating,3)&~cov
        semantic_target=(coating|padding)&~hard&~fabric
        for label,mask in [('coating',coating),('mechanical',hard),('fabric',fabric),('padding',padding)]:save(folder/f'{key}-{label}.png',mask*255)
        if key=='fgm':save(folder/'fgm-authored-fabric-included.png',authored_fabric*255)
        t=semantic_target.astype(np.float32);c=coating.astype(np.float32);hmap=hard.astype(np.float32);fmap=fabric.astype(np.float32)
        level_checks=[]
        for mip in sorted(levels):
            old=decode_level(original_levels[mip]);level=levels[mip];height,width=old.shape[:2]
            bh,bw=max(1,(height+3)//4),max(1,(width+3)//4)
            def blockstat(v,kind):
                v=np.pad(v,((0,bh*4-height),(0,bw*4-width)))
                q=v.reshape(bh,4,bw,4)
                return (q.max((1,3)) if kind=='max' else q.mean((1,3))).ravel()
            ts=blockstat(t,'mean');cs=blockstat(c,'mean');hs=blockstat(hmap,'mean');fs=blockstat(fmap,'mean')
            hm=blockstat(hmap,'max');fm=blockstat(fmap,'max')
            strict=(ts>0)&(hm==0)&(fm==0)
            mixed=np.zeros_like(strict)
            if mip>=3:
                # Only unresolved protected fractions may join a coating-dominant mip.
                mixed=(ts>0)&(cs/np.maximum(cs+hs+fs,1e-12)>=2/3)&(hm<.25)&(hs<=.05)&(fm<.5)
            selected=strict|mixed
            payload=np.frombuffer(level['payload'],np.uint8).reshape(-1,16).copy();source_payload=payload.copy();modes={}
            for i in np.flatnonzero(selected):
                newblock,mode=neutral_bc7_blue(payload[i].tobytes());payload[i]=np.frombuffer(newblock,np.uint8)
                modes[str(mode)]=modes.get(str(mode),0)+1
            replace_level(level,payload.tobytes());new=decode_level(level)
            pixelmask=selected.reshape(bh,bw).repeat(4,0).repeat(4,1)[:height,:width]
            assert np.array_equal(old[...,[0,1,3]],new[...,[0,1,3]]),(key,mip,'RGA changed')
            assert np.array_equal(old[~pixelmask],new[~pixelmask]),(key,mip,'Outside pixels changed')
            assert np.array_equal(source_payload[~selected],payload[~selected])
            gate=weather_gate(new[...,2][pixelmask])
            if pixelmask.any():assert float(gate.max())<1e-10,(key,mip,'Non-neutral selected gate')
            detail_old=np.minimum(np.floor(15*old[...,2].astype(float)/255+.5),15)
            detail_new=np.minimum(np.floor(15*new[...,2].astype(float)/255+.5),15)
            assert np.array_equal(detail_old[~pixelmask],detail_new[~pixelmask])
            if pixelmask.any():assert np.all(detail_new[pixelmask]==8)
            resolved_hard=hmap>=.25;resolved_fabric=fmap>=.5
            assert not np.any(pixelmask&resolved_hard),(key,mip,'Resolved mechanical region touched')
            assert not np.any(pixelmask&resolved_fabric),(key,mip,'Resolved fabric region touched')
            unhandled_coating=float(c[~pixelmask].sum())
            record={'mip':mip,'size':[width,height],'selected_blocks':int(selected.sum()),'total_blocks':int(selected.size),
                    'mixed_blocks':int((selected&~strict).sum()),'selected_pixels':int(pixelmask.sum()),
                    'coating_footprint_total':float(c.sum()),'coating_footprint_neutralized':float(c[pixelmask].sum()),
                    'coating_footprint_retained_at_protected_interfaces':unhandled_coating,
                    'mechanical_footprint_in_mixed_blocks':float(hmap[pixelmask].sum()),
                    'fabric_footprint_in_mixed_blocks':float(fmap[pixelmask].sum()),
                    'RGA_exact':True,'outside_pixels_blocks_and_detail_index_exact':True,'resolved_protected_pixels_untouched':True,
                    'W_after_max':float(gate.max()) if pixelmask.any() else None,
                    'new_B_range':[int(new[...,2][pixelmask].min()),int(new[...,2][pixelmask].max())] if pixelmask.any() else [],'modes':modes}
            level_checks.append(record)
            save(folder/f'{key}-target-mip{mip}.png',pixelmask*255)
            save(OUT/'verify'/f'{key}_n_mip{mip}.png',new)
            if mip==0:
                overlay=a.copy();overlay[pixelmask,:3]=(overlay[pixelmask,:3]*.4+np.array([30,220,90])*.6).astype('uint8')
                overlay[hard,:3]=[255,60,40];overlay[fabric,:3]=[180,90,245]
                save(folder/f'{key}-selection-overlay.png',overlay)
                contact(key+'-weather-final',[np.repeat(old[...,2:3],3,-1),np.repeat(new[...,2:3],3,-1),overlay],['PRE-WEATHER NORMAL B','FINAL COATING B','GREEN TARGET / RED MECHANICAL / VIOLET FABRIC'])
            if max(height,width)>1:
                def down(v):
                    v=np.pad(v,((0,height%2),(0,width%2)))
                    return v.reshape(v.shape[0]//2,2,v.shape[1]//2,2).mean((1,3))
                t,c,hmap,fmap=[down(v) for v in (t,c,hmap,fmap)]
        checks[key]={'coating_pixels':int(coating.sum()),'mechanical_pixels':int(hard.sum()),'fabric_pixels':int(fabric.sum()),'authored_fabric_included_pixels':int(authored_fabric.sum()),'padding_pixels':int(padding.sum()),'levels':level_checks}
    allowed_stems={NAMES[k]+'_n' for k in keys}
    for p in (STAGE/'assets').rglob('*'):
        if not p.is_file():continue
        rel=str(p.relative_to(STAGE)).replace('\\','/');old=baseline[rel];digest=sha(p)
        if digest!=old['sha256']:
            assert p.stem in allowed_stems and p.suffix in ('.dds','.dd0','.dd1','.dd2'),('Unrelated resource changed',rel)
            incremental.append({'path':rel,'original_sha256':old['sha256'],'refined_sha256':digest,'size':p.stat().st_size})
    main_manifest=json.loads((OUT/'patch-manifest.json').read_text(encoding='utf-8'));by_path={e['path']:e for e in incremental}
    assert set(by_path)<=set(e['path'] for e in main_manifest['files'])
    for entry in main_manifest['files']:
        if entry['path'] in by_path:entry['refined_sha256']=by_path[entry['path']]['refined_sha256'];entry['size']=by_path[entry['path']]['size']
    dump(OUT/'patch-manifest.json',main_manifest)
    proof={'keys':list(keys),'files_changed':len(incremental),'only_normal_B_changed':True,'all_other_assets_hash_identical':True,
           'detail_target':8,'detail_requires_all_influences_zero':True,'materials_checked':len(materials),'visuals_checked':len(visual_bindings),
           'coarse_rule':'mip>=3; coating/(coating+mechanical+fabric)>=2/3; max mechanical texel<.25, block mechanical mean<=.05; max fabric texel<.5',
           'resolved_mechanical_pixels_preserved':True,'authored_navy_covers_intentionally_dry_with_RGA_exact':True,'checks':checks}
    dump(folder/'verification.json',proof)
    dump(folder/'patch-manifest.json',{'source':'pre-weather texture-stage; rollback hashes identify the integrated refined baseline','stage':str(STAGE),'rollback':str(folder/'rollback'),'files':incremental})
    verification=json.loads((OUT/'verification.json').read_text(encoding='utf-8'));verification['final_weather_B_cleanup']=proof
    verification['previous_normal_B_preservation_superseded']='Intentional target B semantic change after positive real-port A/B. A/R/G remain exact to pre-weather refined state; non-target B/detail remain exact except explicitly quantified unresolved coarse mixtures.'
    for key in keys:
        verification['pixel_checks'][key]['n']['scope']='Pre-weather codec measurement; final intentional B semantics and exact R/G/A relative to this baseline are verified in final_weather_B_cleanup.'
    dump(OUT/'verification.json',verification)
    reports=json.loads((OUT/'dds-info.json').read_text(encoding='utf-8'))
    for key in keys:
        name=NAMES[key]+'_n.dds'
        reports[name]['after']=cli('tex','info',name,'--project',STAGE)['texture']
        reports[name]['final_weather_semantics']='Target B is intentionally neutralized; all R/G/A and unselected blocks remain exact to pre-weather refined baseline.'
    dump(OUT/'dds-info.json',reports)
    print(json.dumps({'files_changed':len(incremental),'manifest_files':len(main_manifest['files']),'verification':str(folder/'verification.json')}),flush=True)

def guard_all_mips(keys):
    """Use original B/A at each mip, never infer lower-mip semantics from mip 0.

    Normal lower mips are encoded independently from cleaned R/G + that source
    mip's B/A. Rare remaining semantic/tail outliers retain original BC blocks.
    MG restores source blocks wherever B/A differs; paint R/G elsewhere stays clean.
    """
    report_path=OUT/'dds-info.json';reports=json.loads(report_path.read_text(encoding='utf-8'))
    checks={}
    for key in keys:
        for ch in ('n','mg'):
            name=NAMES[key]+f'_{ch}.dds'
            if name not in reports:continue
            src=original_dds(key,ch);dst=STAGE/source_path(key,ch).relative_to(SOURCE)
            original=dds_levels(src);current=dds_levels(dst);assert original.keys()==current.keys()
            checks[name]=[]
            for mip in sorted(original):
                old=decode_level(original[mip]);level=current[mip];actual=decode_level(level)
                exact_before=np.array_equal(old[...,2:],actual[...,2:])
                independently_encoded=False
                if ch=='n' and mip>0 and not exact_before:
                    work=OUT/'mip-work'/key;work.mkdir(parents=True,exist_ok=True)
                    rgba=actual.copy();rgba[...,2:]=old[...,2:]
                    png=work/f'mip{mip}.png';save(png,rgba)
                    encoded=work/f'mip{mip}.dds'
                    cli('tex','import',png,'--like',src,'-o',encoded,'--quality','high','--mip-filter','linear')
                    first=dds_levels(encoded)[0]
                    assert (first['width'],first['height'])==(level['width'],level['height'])
                    replace_level(level,first['payload']);actual=decode_level(level)
                    independently_encoded=True
                delta=np.abs(old[...,2:].astype(np.int16)-actual[...,2:].astype(np.int16))
                alpha_shift=np.rint(old[...,3].astype(float)/85)!=np.rint(actual[...,3].astype(float)/85)
                if ch=='mg':bad=np.any(delta!=0,axis=-1)
                else:bad=(delta[...,0]>8)|(delta[...,1]>3)|alpha_shift
                h,w=bad.shape;bh,bw=max(1,(h+3)//4),max(1,(w+3)//4)
                padded=np.pad(bad,((0,bh*4-h),(0,bw*4-w)))
                blocks=padded.reshape(bh,4,bw,4).max((1,3)).ravel()
                if blocks.any():
                    payload=np.frombuffer(level['payload'],np.uint8).reshape(-1,level['block_size']).copy()
                    source_payload=np.frombuffer(original[mip]['payload'],np.uint8).reshape(payload.shape)
                    payload[blocks]=source_payload[blocks]
                    replace_level(level,payload.tobytes());actual=decode_level(level)
                finaldelta=np.abs(old[...,2:].astype(np.int16)-actual[...,2:].astype(np.int16))
                finalshift=np.rint(old[...,3].astype(float)/85)!=np.rint(actual[...,3].astype(float)/85)
                assert not finalshift.any(),(name,mip,'alpha class')
                if ch=='mg':assert not finaldelta.any(),(name,mip,'emission B/A')
                else:assert finaldelta[...,0].max()<=8 and finaldelta[...,1].max()<=3,(name,mip,'B/A tail')
                record={'mip':mip,'size':[w,h],'independent_normal_encode':independently_encoded,
                        'protected_blocks':int(blocks.sum()),'total_blocks':int(blocks.size),'protected_fraction':float(blocks.mean()),
                        'BA_max':finaldelta.max((0,1)).tolist(),'BA_mean':finaldelta.mean((0,1)).tolist(),
                        'alpha_class_changes':int(finalshift.sum()),'BA_exact':bool(not finaldelta.any())}
                checks[name].append(record)
            reports[name]['after']=cli('tex','info',name,'--project',STAGE)['texture']
            for mip in range(min(4,len(current))):cli('tex','export',name,'--project',STAGE,'--mip',mip,'-o',OUT/'verify'/f'{key}_{ch}_mip{mip}.png')
            print(name+' all '+str(len(current))+' source-mip B/A checks passed',flush=True)
    dump(OUT/'mip-semantic-verification.json',checks);dump(report_path,reports)

def verify(keys):
    source_manifest=json.loads((OUT/'source-manifest.json').read_text(encoding='utf-8'))
    changes=[]
    for p in (STAGE/'assets').rglob('*'):
        if not p.is_file():continue
        rel=str(p.relative_to(STAGE)).replace('\\','/')
        orig=source_manifest.get(rel); digest=sha(p)
        if not orig or orig['sha256']!=digest:
            assert p.suffix in ['.dds','.dd0','.dd1','.dd2','.mfm'],rel
            changes.append({'path':rel,'refined_sha256':digest,'original_sha256':orig['sha256'] if orig else None,'size':p.stat().st_size})
    for rel,info in source_manifest.items(): assert sha(SOURCE/rel)==info['sha256'],'Source mutated: '+rel
    stats={}
    for key in keys:
        stats[key]={}
        masks=np.load(OUT/'masks'/f'{key}.npz')
        for ch in ['a','n','mg','ao']:
            p=OUT/'verify'/f'{key}_{ch}_mip0.png'
            if not p.exists():continue
            actual=image(p); planned=image(OUT/'edited'/f'{key}_{ch}.png'); original=raw(key,ch)
            err=np.abs(actual.astype(np.int16)-planned.astype(np.int16))
            clean=~masks['changed_'+ch]
            stats[key][ch]={'compression_mae_rgba':err.mean((0,1)).tolist(),'compression_p99_rgba':np.percentile(err,99,axis=(0,1)).tolist(),
                'unchanged_region_mae_rgba':np.abs(actual[clean].astype(float)-original[clean]).mean(0).tolist() if clean.any() else None}
            if ch in ('n','mg'):
                delta=np.abs(actual[...,2:].astype(np.int16)-original[...,2:].astype(np.int16))
                stats[key][ch]['BA_mae']=delta.mean((0,1)).tolist()
                stats[key][ch]['BA_p99']=np.percentile(delta,99,axis=(0,1)).tolist()
                noop=image(OUT/'verify'/f'{key}_{ch}_noop.png')
                noop_delta=np.abs(noop[...,2:].astype(np.int16)-original[...,2:].astype(np.int16))
                stats[key][ch]['noop_BA_mae']=noop_delta.mean((0,1)).tolist()
                stats[key][ch]['noop_BA_p99']=np.percentile(noop_delta,99,axis=(0,1)).tolist()
                if ch=='mg':
                    assert np.array_equal(actual[...,2:],original[...,2:]),key+' compressed emission mask changed'
                    stats[key][ch]['compressed_BA_exact']=True
        if key in ['hull','dh','fgm']:
            game=raw(key,'mg',True); original=raw(key,'mg')
            stats[key]['mg_current_vs_game_MAE']=np.abs(original.astype(float)-game.astype(float)).mean((0,1)).tolist()
    materials=[]
    for p in (STAGE/'assets').rglob('*.mfm'):
        if not p.name.startswith(tuple(NAMES.values())):continue
        src=SOURCE/p.relative_to(STAGE); old=src.read_text(); new=p.read_text()
        assert re.findall(r'<Texture>(.*?)</Texture>',old,re.S)==re.findall(r'<Texture>(.*?)</Texture>',new,re.S),'Texture references changed'
        details={}
        for prop in ('g_detailAlbedoInfluence','g_detailGlossInfluence','g_detailNormalInfluence'):
            m=re.search(r'<property>\s*'+prop+r'\s*<Float>\s*([\d.]+)',new)
            if m:details[prop]=float(m[1]);assert float(m[1])==0,p
        materials.append({'path':str(p.relative_to(STAGE)),'detail':details,'texture_references_unchanged':True})
    # Decode every mip, not just the four presentation levels, and check dimensions.
    infos=json.loads((OUT/'dds-info.json').read_text(encoding='utf-8'))
    jobs=[]; semantic_sources={};semantic_outputs={}
    for name,report in infos.items():
        key=next(k for k,prefix in NAMES.items() if name.startswith(prefix+'_'))
        if key not in keys:continue
        ch=name[len(NAMES[key])+1:-4]; info=report['after']
        if ch in ('n','mg'):
            semantic_sources[name]=dds_levels(original_dds(key,ch))
            semantic_outputs[name]=dds_levels(STAGE/source_path(key,ch).relative_to(SOURCE))
        for mip in range(info['levels']):jobs.append((key,ch,name,mip,info))
    def check_mip(job):
        key,ch,name,mip,info=job
        target=OUT/'verify'/f'{key}_{ch}_mip{mip}.png'
        if mip>=4:cli('tex','export',name,'--project',STAGE,'--mip',mip,'-o',target)
        actual=Image.open(target).size;expected=(max(1,info['width']>>mip),max(1,info['height']>>mip))
        assert actual==expected,(name,mip,actual,expected)
        check={'texture':name,'mip':mip,'dimensions':list(actual)}
        if ch in ('n','mg'):
            old=decode_level(semantic_sources[name][mip]);new=decode_level(semantic_outputs[name][mip])
            delta=np.abs(old[...,2:].astype(np.int16)-new[...,2:].astype(np.int16))
            shift=np.rint(old[...,3].astype(float)/85)!=np.rint(new[...,3].astype(float)/85)
            assert not shift.any(),(name,mip,'alpha class drift')
            if ch=='mg':assert not delta.any(),(name,mip,'MG B/A changed')
            else:assert delta[...,0].max()<=8 and delta[...,1].max()<=3,(name,mip,'normal B/A outlier')
            check.update(BA_max=delta.max((0,1)).tolist(),BA_exact=bool(not delta.any()),alpha_class_changes=int(shift.sum()))
        return check
    with ThreadPoolExecutor(max_workers=4) as pool: mip_checks=list(pool.map(check_mip,jobs))
    mip_count=len(mip_checks)
    dump(OUT/'verification.json',{'source_unchanged':True,'changed_files':len(changes),'pixel_checks':stats,
         'material_checks':materials,
         'mip_exports_checked':mip_count,
         'mip_checks':mip_checks,
         'visual_review':'pending independent visual review; no game validation claimed'})
    dump(OUT/'patch-manifest.json',{'source':str(SOURCE),'stage':str(STAGE),'files':changes})
    print(json.dumps({'patch_files':len(changes),'verification':str(OUT/'verification.json')}))

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--phase',choices=['prepare','process','import','guard','mipguard','verify','secondary','weather-probe','weather-final'],required=True)
    p.add_argument('--keys',nargs='+',choices=list(NAMES),default=list(NAMES))
    args=p.parse_args()
    for folder in ['inputs','edited','masks','evidence','verify','noop']: (OUT/folder).mkdir(parents=True,exist_ok=True)
    if args.phase=='prepare':prepare()
    elif args.phase=='process':
        for key in args.keys:process(key)
    elif args.phase=='import':import_images(args.keys)
    elif args.phase=='guard':guard_normal_semantics(args.keys)
    elif args.phase=='mipguard':guard_all_mips(args.keys)
    elif args.phase=='secondary':secondary_collar_revision()
    elif args.phase=='weather-probe':weather_probe()
    elif args.phase=='weather-final':weather_final()
    else:verify(args.keys)
if __name__=='__main__': main()
