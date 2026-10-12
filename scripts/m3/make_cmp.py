# 生成每类改动的改前/改后拼图 -> ./work/m3/ingame/
from PIL import Image, ImageDraw, ImageFont, ImageChops
R='./work/m3/ingame/raw/'; OUT='./work/m3/ingame/'
F=ImageFont.truetype('/usr/share/fonts/opentype/noto/NotoSerifCJK-Bold.ttc',26)
def L(p): return Image.open(R+p).convert('RGB')
def pair(name,title,items,box,scale=1.0):
    # items: [(标签, 文件, dx)] dx 为该截图相对第一张的水平偏移(相机略有差异时对齐用)
    crops=[]
    for lab,p,dx in items:
        b=(box[0]+dx,box[1],box[2]+dx,box[3]); c=L(p).crop(b)
        if scale!=1.0: c=c.resize((int(c.width*scale),int(c.height*scale)),Image.LANCZOS)
        crops.append((lab,c))
    w=max(c.width for _,c in crops); h=crops[0][1].height
    horiz = w*len(crops) <= 2200
    W = w*len(crops)+10*(len(crops)-1) if horiz else w
    H = (h+40) if horiz else (h+40)*len(crops)
    o=Image.new('RGB',(W,H+50),'white'); d=ImageDraw.Draw(o); d.text((10,8),title,fill='black',font=F)
    for i,(lab,c) in enumerate(crops):
        x,y=((i*(w+10),50) if horiz else (0,50+i*(h+40)))
        d.text((x+6,y+2),lab,fill='black',font=F); o.paste(c,(x,y+36))
    o.save(OUT+name); print(name,o.size)
pair('m3_cmp_rebuild_equivalent.png','等价重建: 左=原样(base) 右=编码器重建(rebuild), 侧视与近景',
     [('改前 base v2','base_v2_side_small.png',0),('改后 rebuild v2','rebuild_v2_side_small.png',0)],(380,300,1280,520))
pair('m3_cmp_rebuild_equivalent_near.png','等价重建 近景(舯部副炮): 上=base 下=rebuild',
     [('改前 base v4','base_v4_near_small.png',0),('改后 rebuild v4','rebuild_v4_near_small.png',0)],(380,40,1280,600))
pair('m3_cmp_scale_hp.png','缩放(挂点): HP_FGM_3 后主炮 ×1.5 (combo 中)',
     [('改前 base','base2/base_x15_small.png',0),('改后 combo','combo_x15_small.png',0)],(375,150,1280,600))
pair('m3_cmp_scale_part.png','缩放(部件几何): FGM049 全部主炮 ×1.5 (scalepart; 港口光照随时间变为黄昏)',
     [('改前 base','base2/base_x15_small.png',0),('改后 scalepart','scalepart/scalepart_x15_small.png',0)],(375,150,1280,600))
pair('m3_cmp_move.png','移动: HP_FGS_8/9 (后部 127mm 双联副炮) 上移 0.25 (move 项目, 侧视放大 2×)',
     [('改前 base','base2/base_v2_side_small.png',0),('改后 move','move/move_v2_side_small.png',0)],(560,330,1100,450),2.0)
pair('m3_cmp_replace.png','替换: FGS036 (127mm 双联) → FGS053 (152mm 三联) 外形 (combo, 侧视放大 2×)',
     [('改前 base','base2/base_v2_side_small.png',0),('改后 combo','combo_v2_side_small.png',0)],(560,330,1100,450),2.0)
pair('m3_cmp_remove.png','删除: MidBack 渲染集 Chimney (烟囱) (remove, 放大 2.5×)',
     [('改前 base','base2/base_x14_small.png',0),('改后 remove','remove/remove_x14_small.png',-21)],(581,200,801,400),2.5)
pair('m3_cmp_newmesh.png','新网格: 烟囱顶盖+桅杆 (funnel_cap.glb, 260 顶点; combo, 放大 2.5×)',
     [('改前 base','base2/base_x14_small.png',0),('改后 combo','combo_x14_small.png',-21)],(581,200,801,400),2.5)
pair('m3_cmp_blender.png','Blender 往返: 3 个烟囱对象竖向 ×1.35 (blender, 放大 2.5×)',
     [('改前 base','base2/base_x14_small.png',0),('改后 blender','blender/blender_x14_small.png',0)],(581,200,801,400),2.5)
pair('m3_lod_far.png','远景 x16 (LOD)',
     [('base','base2/base_x16_small.png',0),('combo','combo_x16_small.png',0),('remove','remove/remove_x16_small.png',0),('blender','blender/blender_x16_small.png',0)],(560,250,1200,520))
# 等价重建的数值差: 船体区域平均像素差(港口水面/光照是动态的, 仅供参考)
for v,box in [('v2_side',(380,330,1280,470)),('v4_near',(380,40,1280,560))]:
    a=L(f'base_{v}_small.png').crop(box); b=L(f'rebuild_{v}_small.png').crop(box)
    st=ImageChops.difference(a,b).convert('L'); h=st.histogram(); n=sum(h)
    print(v,'mean',round(sum(i*c for i,c in enumerate(h))/n,2),'>30:',round(sum(h[31:])/n*100,2),'%')
