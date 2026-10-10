import json, struct, sys, numpy as np
def glb(p):
    d=open(p,'rb').read(); jl=struct.unpack('<I',d[12:16])[0]; j=json.loads(d[20:20+jl]); b=d[20+jl+8:]
    def acc(i):
        a=j['accessors'][i]; v=j['bufferViews'][a['bufferView']]; off=v.get('byteOffset',0)+a.get('byteOffset',0)
        n={'SCALAR':1,'VEC2':2,'VEC3':3,'MAT4':16}[a['type']]; dt={5126:'<f4',5125:'<u4',5123:'<u2'}[a['componentType']]
        return np.frombuffer(b,dtype=dt,count=a['count']*n,offset=off).reshape(a['count'],n)
    return j,acc
def summary(p):
    j,acc=glb(p); out={}
    for n in j['nodes']:
        m=np.array(n.get('matrix',list(np.eye(4).T.flatten()))).reshape(4,4).T
        prims=[]
        if 'mesh' in n:
            for pr in j['meshes'][n['mesh']]['primitives']:
                prims.append((acc(pr['attributes']['POSITION']), acc(pr['indices']).flatten()))
        out[n['name']]=(m,prims)
    return out
a=summary(sys.argv[1]); b=summary(sys.argv[2])
print('nodes', len(a), len(b), 'same names', set(a)==set(b))
for k in sorted(set(a)|set(b)):
    if k not in a or k not in b: print('  missing', k, k in a, k in b); continue
    (ma,pa),(mb,pb)=a[k],b[k]
    dm=np.abs(ma-mb).max(); 
    dp=max([np.abs(x[0]-y[0]).max() for x,y in zip(pa,pb)] or [0]); di=all(np.array_equal(x[1],y[1]) for x,y in zip(pa,pb))
    if dm>1e-4 or dp>1e-5 or not di or len(pa)!=len(pb): print('  DIFF',k,'mat',dm,'pos',dp,'idx',di,len(pa),len(pb))
print('max node matrix diff', max(np.abs(a[k][0]-b[k][0]).max() for k in a if k in b))
ta=json.load(open(sys.argv[3])); tb=json.load(open(sys.argv[4]))
ma={m['hp']:m for m in ta['mounts']}; mb={m['hp']:m for m in tb['mounts']}
print('mounts', len(ma), len(mb), set(ma)==set(mb))
worst=0
for h in ma:
    if h in mb:
        for f in ['game_world_matrix','gltf_pos','gltf_yaw_deg','game_barrel_dir']:
            worst=max(worst, float(np.abs(np.array(ma[h][f])-np.array(mb[h][f])).max()))
        for f in ['model','section','numBarrels','module']:
            if ma[h].get(f)!=mb[h].get(f): print('  field', h, f, ma[h].get(f), mb[h].get(f))
print('max mount numeric diff', worst)
print('sections', ta['sections']==tb['sections'])
