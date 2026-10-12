#!/usr/bin/env python3
"""UV 位置图烘焙: 把 GLB 某材质的三角形按 UV 光栅化, 每个贴图像素记录世界坐标(xyz)+法线+覆盖。
用途: 在 UV 空间按「世界位置/朝向」画图案(舷侧弧块沿船长、水线按高度、甲板按法线朝上), 线条自然贴结构。
用法: uvpos_bake.py geometry.glb --material 'Hull\\.mfm$' --size 2048 -o hull_pos.npz
输出 npz: pos(H,W,3) float32, nrm(H,W,3), cov(H,W) uint8(覆盖次数; >1 = UV 复用/镜像), uvrange
注意: GLB 坐标 y 上, z 沿船长(艏+z), x 横向。UV 复用(左右舷共用)时后写覆盖先写; 用 |x| 设计对称图案。
"""
import argparse, json, struct, re
import numpy as np

def load_glb(p):
    d=open(p,'rb').read(); n=struct.unpack_from('<I',d,12)[0]; j=json.loads(d[20:20+n])
    off=20+n; blen=struct.unpack_from('<I',d,off)[0]; b=d[off+8:off+8+blen]
    return j,b

CT={5126:(np.float32,4),5123:(np.uint16,2),5125:(np.uint32,4),5121:(np.uint8,1)}
NC={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4,'MAT4':16}
def acc(j,b,i):
    a=j['accessors'][i]; bv=j['bufferViews'][a['bufferView']]
    dt,sz=CT[a['componentType']]; nc=NC[a['type']]; o=bv.get('byteOffset',0)+a.get('byteOffset',0)
    st=bv.get('byteStride',0) or sz*nc
    if st==sz*nc:
        return np.frombuffer(b,dt,a['count']*nc,o).reshape(a['count'],nc).astype(np.float64 if dt==np.float32 else np.int64)
    raw=np.frombuffer(b,np.uint8,st*a['count'],o).reshape(a['count'],st)[:,:sz*nc].copy()
    return raw.view(dt).reshape(a['count'],nc).astype(np.float64 if dt==np.float32 else np.int64)

def qmat(q):
    x,y,z,w=q
    return np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],[2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],[2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])
def nodemat(n):
    if 'matrix' in n: return np.array(n['matrix']).reshape(4,4).T
    M=np.eye(4); R=qmat(n.get('rotation',[0,0,0,1])); S=np.diag(n.get('scale',[1,1,1]))
    M[:3,:3]=R@S; M[:3,3]=n.get('translation',[0,0,0]); return M

def world_mats(j):
    out={}; par={}
    for i,n in enumerate(j['nodes']):
        for c in n.get('children',[]): par[c]=i
    def W(i):
        if i in out: return out[i]
        M=nodemat(j['nodes'][i]); 
        if i in par: M=W(par[i])@M
        out[i]=M; return M
    for i in range(len(j['nodes'])): W(i)
    return out

def bake(glb,mat_re,size,wrap,node_re=None,local=False):
    j,b=load_glb(glb); mats=[m.get('name','') for m in j.get('materials',[])]; rx=re.compile(mat_re)
    WM=world_mats(j)
    pos=np.zeros((size,size,3),np.float32); nrm=np.zeros((size,size,3),np.float32); cov=np.zeros((size,size),np.uint8); upc=np.zeros((size,size),np.uint8); wrapc=np.zeros((size,size),np.uint8)
    urng=[1e9,-1e9,1e9,-1e9]; ntri=0
    for ni,n in enumerate(j['nodes']):
        if 'mesh' not in n: continue
        if node_re and not re.search(node_re,n.get('name','')): continue
        M=np.eye(4) if local else WM[ni]; R=M[:3,:3]
        for p in j['meshes'][n['mesh']]['primitives']:
            if 'material' not in p or not rx.search(mats[p['material']]): continue
            A=p['attributes']; P=acc(j,b,A['POSITION']); N=acc(j,b,A['NORMAL']) if 'NORMAL' in A else np.zeros_like(P)
            UV=acc(j,b,A['TEXCOORD_0']); I=acc(j,b,p['indices']).reshape(-1,3)
            Pw=(P@R.T)+M[:3,3]; Nw=N@R.T; Nw/=np.linalg.norm(Nw,axis=1,keepdims=True)+1e-9
            urng=[min(urng[0],UV[:,0].min()),max(urng[1],UV[:,0].max()),min(urng[2],UV[:,1].min()),max(urng[3],UV[:,1].max())]
            uv=UV.copy()
            if wrap: uv=uv-np.floor(uv.mean(1,keepdims=True)*0)  # keep; tiled handled by caller
            px=uv*size-0.5
            for t in I:
                a,bq,c=px[t].copy()
                # 平铺 UV(超出 0..1, 如甲板木纹带): 平移到主周期再光栅, 像素按 size 取模(repeat 采样)。旧版直接裁掉 → 这些三角的纹素位置记成别的面, 出现甲板白楔
                sh=np.floor(np.minimum(np.minimum(a,bq),c)/size)*size; a=a-sh; bq=bq-sh; c=c-sh; wrapped=bool(np.any(sh!=0))
                x0=int(np.floor(min(a[0],bq[0],c[0]))); x1=int(min(np.ceil(max(a[0],bq[0],c[0])),x0+2*size))
                y0=int(np.floor(min(a[1],bq[1],c[1]))); y1=int(min(np.ceil(max(a[1],bq[1],c[1])),y0+2*size))
                if x1<x0 or y1<y0: continue
                xs,ys=np.meshgrid(np.arange(x0,x1+1),np.arange(y0,y1+1))
                v0=bq-a; v1=c-a; den=v0[0]*v1[1]-v1[0]*v0[1]
                if abs(den)<1e-12: continue
                dx=xs-a[0]; dy=ys-a[1]
                l1=(dx*v1[1]-v1[0]*dy)/den; l2=(v0[0]*dy-dx*v0[1])/den; l0=1-l1-l2
                e=-1e-3*0+ -0.02
                m=(l0>=e)&(l1>=e)&(l2>=e)
                if not m.any(): continue
                L=np.stack([l0[m],l1[m],l2[m]],1)
                yy,xx=ys[m]%size,xs[m]%size
                pos[yy,xx]=L@Pw[t]; nn=L@Nw[t]; nrm[yy,xx]=nn/ (np.linalg.norm(nn,axis=1,keepdims=True)+1e-9)
                cov[yy,xx]=np.minimum(cov[yy,xx].astype(int)+1,255)
                if (Nw[t][:,1].mean()>0.65): upc[yy,xx]=np.minimum(upc[yy,xx].astype(int)+1,255)  # 朝上(甲板)命中次数: 与舷侧共用纹素时甲板优先
                if wrapped: wrapc[yy,xx]=np.minimum(wrapc[yy,xx].astype(int)+1,255)
                ntri+=1
    return pos,nrm,cov,urng,ntri,upc,wrapc

if __name__=='__main__':
    ap=argparse.ArgumentParser(); ap.add_argument('glb'); ap.add_argument('--material',required=True)
    ap.add_argument('--size',type=int,default=1024); ap.add_argument('-o',required=True)
    ap.add_argument('--node',help='节点名正则(共享贴图的多实例只烘一个)'); ap.add_argument('--local',action='store_true',help='用网格局部坐标(炮塔等)')
    a=ap.parse_args()
    pos,nrm,cov,urng,nt,upc,wrapc=bake(a.glb,a.material,a.size,False,a.node,a.local)
    np.savez_compressed(a.o,pos=pos,nrm=nrm,cov=cov,upc=upc,wrapc=wrapc,uvrange=np.array(urng))
    print(json.dumps(dict(out=a.o,tris=nt,uvrange=[round(float(x),3) for x in urng],covered=float((cov>0).mean()),multi=float((cov>1).mean()),up=float((upc>0).mean()),wrapped=float((wrapc>0).mean()))))
