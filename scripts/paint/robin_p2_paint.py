#!/usr/bin/env python3
"""知更鸟 P2 贴图层画师(世界坐标驱动, 图案贴结构走)。
输入: uvpos_bake 的 hull_pos.npz / dh_pos.npz + 原版 _a PNG。输出: 4096 新 _a PNG(屏上色经港口标定换算)、色号索引图、侧视预览。
图案来源(官方立绘, 仅取母题几何, 非贴人像):
  - 甲板徽记 光环+耳羽+八芒星: 知更鸟立绘.png(头顶金环、耳侧白羽、裙摆八芒星)
  - 舷侧五线谱金线+音符: 知更鸟•晴歌立绘.png(金色五线谱缎带)
  - 深色块星点: 知更鸟立绘.png(紫前摆星空纹)
  - 缎带弧块: 知更鸟立绘.png 宝蓝缎带/长发流线
"""
import argparse, json, math
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

# 屏上目标色(P2 色板, 见 color_derive/robin_p2_scheme.json)
PAL = {
 'white':  '#F4F0E8', 'peri': '#A8C2F2', 'peri_d': '#8CA3E6', 'navy': '#243A78', 'deck': '#C9CEE0',
 'dhwhite':'#E8E4EE', 'turret':'#F5F2F8', 'violet':'#8B70E0', 'glow':'#58B8FF', 'gold':'#E0C070',  # P5.1: peri 回鲜明紫蓝; navy 细靴仍饱和; violet 更清透; 金略暖
 'roof':   '#E9ECF5', 'star': '#F2EDDF', 'royal': '#573C9F',
}
# P5: deck(多面共用纹素)由深紫 #2A1C55 → 浅冷灰, 否则俯视出现大块深色; periwinkle 向清透蓝(呼应发光 #58B8FF), 值=港口期望色, 贴图走 port 全补偿; roof(甲板/顶面底)提到冷白(甲板是俯视大面)
NAMES=list(PAL)
def hx(h): return np.array([int(h[i:i+2],16) for i in (1,3,5)],float)

# 港口标定(3 点重拟合, r20): 线性 y = a(x - t)
SLOPE=np.array([0.7091,1.1475,1.1523]); TOE=np.array([0.0249,0.0294,0.0137])
def lin(v): v=v/255; return np.where(v>0.04045,((v+0.055)/1.055)**2.4,v/12.92)
def srgb(l): l=np.clip(l,0,1); return np.where(l>0.0031308,1.055*l**(1/2.4)-0.055,12.92*l)*255
# 每色补偿方式: 白系用原值(港口自然偏冷白, 合知更鸟冷白); periwinkle/紫 半补偿; 金用偏浅原值防橙
# v5 港口实测(r21): 金 #E6CF8E(raw) 港口成黄绿 #C2D993 —— 港口 G/B 增益 > R, 金必须全补偿(贴图偏橙 #FEB266 → 港口 ≈#D8B868);
# 舷侧暖白 raw 在港口偏薄荷灰 → 半补偿(贴图略偏暖粉)。炮塔/上层白在港口高光截断, 仍用 raw。
COLMODE={'white':'half','dhwhite':'raw','turret':'raw','roof':'raw','star':'half','navy':'raw','deck':'raw',
         'peri':'port','peri_d':'port','royal':'half','violet':'half','glow':'raw','gold':'port'}   # c2: 发光天青用原值(港口 G/B 增益只会更亮更青)
TEXOVR={}
STYLE='stripes'
def to_tex(rgb,mode):
    if mode=='raw': return rgb
    if mode=='half':  # 屏/贴折中: 工作台与港口都不离谱
        return 0.5*rgb+0.5*srgb(TOE+lin(rgb)/SLOPE)
    return srgb(TOE+lin(rgb)/SLOPE)

def smooth(a,b,x):
    t=np.clip((x-a)/(b-a+1e-9),0,1); return t*t*(3-2*t)

# ---------- 徽记(光环+耳羽+八芒星), 画在局部 1024 画布, 白=1 金=2 ----------
def octagram(d,cx,cy,r,fill):
    pts=[]
    for k in range(16):
        ang=math.pi/8*k - math.pi/2; rr=r if k%2==0 else r*0.42
        pts.append((cx+rr*math.cos(ang),cy+rr*math.sin(ang)))
    d.polygon(pts,fill=fill)

def emblem(size=1024):
    im=Image.new('L',(size,size),0); d=ImageDraw.Draw(im); c=size/2
    # 金环(两道)
    for r,w in [(300,26),(250,10)]:
        d.ellipse([c-r,c-r*0.42,c+r,c+r*0.42],outline=2,width=w)
    # 耳羽: 每侧 4 片羽, 根在环两端
    for s in (-1,1):
        for k in range(4):
            L=360-k*60; ang=math.radians(-12-k*14)
            bx=c+s*270; by=c+10
            tx=bx+s*L*math.cos(ang); ty=by+L*math.sin(ang)
            nx,ny=-(ty-by),(tx-bx); nn=math.hypot(nx,ny)+1e-9; nx,ny=nx/nn*34,ny/nn*34
            d.polygon([(bx,by),(bx+(tx-bx)*0.5+nx,by+(ty-by)*0.5+ny),(tx,ty),(bx+(tx-bx)*0.5-nx*0.3,by+(ty-by)*0.5-ny*0.3)],fill=1)
    octagram(d,c,c-10,120,1)
    octagram(d,c,c-10,46,2)
    return np.asarray(im)

def sample_decal(img,u,v):
    H,W=img.shape; x=np.clip((u*W).astype(int),0,W-1); y=np.clip((v*H).astype(int),0,H-1)
    inside=(u>=0)&(u<1)&(v>=0)&(v<1); out=np.zeros(u.shape,np.uint8); out[inside]=img[y[inside],x[inside]]; return out

def stars_field(z,y,seed=7,density=1.0,r=0.035):
    """世界坐标稀疏八芒星(近似: 十字+对角菱形), 返回 bool"""
    g=0.42/density; zi=np.floor(z/g); yi=np.floor(y/g)
    h=np.sin(zi*12.9898+yi*78.233+seed)*43758.5453; h=h-np.floor(h)
    cz=(zi+0.2+0.6*((h*7)%1))*g; cy=(yi+0.2+0.6*((h*13)%1))*g
    dz=np.abs(z-cz); dy=np.abs(y-cy); rr=r*(0.6+0.8*((h*3)%1))
    star=((dz*0.25+dy<rr*0.5)|(dy*0.25+dz<rr*0.5)|((dz+dy)<rr*0.45))&(h>0.35)
    return star

# ---------- 船体 ----------
def paint_hull(pos,nrm,cov,upc=None):
    H,W=cov.shape; idx=np.full((H,W),NAMES.index('white'),np.uint8)
    x,y,z=pos[...,0],pos[...,1],pos[...,2]; ax=np.abs(x)
    covd=cov>0
    # 甲板判定: 朝上; 或 y>0.2 且朝下/斜下且非舷侧(|nx|<0.6) —— 甲板 UV 与舱内背面三角共用纹素, 烘焙写入的是背面三角(法线朝下), 不能按法线判成舷侧
    up=covd&((nrm[...,1]>0.65)|((pos[...,1]>0.2)&(nrm[...,1]<-0.2)&(np.abs(nrm[...,0])<0.6)))
    if upc is not None: up|=covd&(upc>0)   # 烘焙记录: 任一朝上三角命中过 → 甲板优先(纹素被舷侧/背面三角复用时)
    side=covd&~up
    zmin,zmax=-8.864,8.80; t=(z-zmin)/(zmax-zmin)   # 0=艏 1=艉
    # 甲板边高度 e(z): 舷侧最高点(按 z 分箱)
    bins=np.linspace(zmin,zmax,90); e=np.zeros(len(bins)-1)
    sz=z[side]; sy=y[side]; sax=ax[side]
    for i in range(len(bins)-1):
        m=(sz>=bins[i])&(sz<bins[i+1])
        if m.any():
            # 外舷: |x| 接近该段最大
            mm=m&(sax>np.percentile(sax[m],60)); e[i]=np.percentile(sy[mm],97) if mm.any() else 0.45
    e=np.clip(e,0.35,0.56); ez=np.interp(z,(bins[:-1]+bins[1:])/2,e)
    f=np.clip(y/ez,-2,2)   # 干舷比例 0=水线 1=甲板边
    # 半宽 hw(z)(甲板五线谱用)
    hw=np.zeros(len(bins)-1)
    for i in range(len(bins)-1):
        m=(sz>=bins[i])&(sz<bins[i+1]); hw[i]=sax[m].max() if m.any() else 0.3
    hwz=np.interp(z,(bins[:-1]+bins[1:])/2,np.maximum(hw,0.15))
    S=side
    # 水线以下 + 靴带
    idx[S&(y<0.045)]=NAMES.index('navy')
    idx[S&(y>=0.045)&(y<0.07)]=NAMES.index('glow')
    hi=S&(y>=0.07)
    # 大弧块: 分界线 b(t)(艏高→舯低→艉起, 长发/缎带流线), 线下 periwinkle、线上白; 金镶边; 下方深色副弧
    # 三段大弧(每段≈舷长1/3, 对标标杆大弧块): 艏斜扫 / 舯拱峰 / 艉上扬
    # v4: 弧块整体抬高(谷底 f≈0.38, 峰 ≈0.9), 侧视主色块占舷侧约一半 —— v3 谷底 0.10 时侧视几乎全白, 弧块读不出
    bow_c=np.where(t<0.34,1.02-(t/0.34)*0.64,-1)
    mid_c=np.where((t>0.30)&(t<0.74),0.38+0.52*np.sin(np.pi*np.clip((t-0.30)/0.44,0,1)),-1)
    st_c=np.where(t>0.70,0.38+0.64*np.clip((t-0.70)/0.27,0,1),-1)
    b=np.clip(np.maximum(np.maximum(bow_c,mid_c),st_c),0.08,1.05)
    band=hi&(f<b)
    idx[band]=NAMES.index('peri')
    b2=b-0.30
    idx[hi&(f<b2)]=NAMES.index('royal')
    edge=hi&(np.abs(f-b)<0.028)
    idx[edge]=NAMES.index('gold')
    edge2=hi&(np.abs(f-b2)<0.012)
    idx[edge2]=NAMES.index('white')
    # P3 发光: 金边内侧(弧块里)一条天青发光线, 隔 0.006 —— 金边本身不发光(发光色=漫反射色, 金发光会糊成橙)
    gl_arc=hi&(f>b-0.068)&(f<b-0.034)&(f>0.10)   # b3: 加宽(0.020→0.034), 远景才读得出
    idx[gl_arc]=NAMES.index('glow')
    # 艏艉暗色楔(标杆黑块对位, 拉 L 跨度) + 星点
    bow=hi&(t<0.16)&(f< 0.62-1.6*(t))
    stern=hi&(t>0.86)&(f< 0.75*(t-0.86)/0.14+0.05)
    for m in (bow,stern): idx[m]=NAMES.index('navy')
    st=stars_field(z,y)&(bow|stern|(S&(y<0.045)&(y>-0.25)))
    idx[st]=NAMES.index('star')
    # 五线谱: 平行于甲板边(f≈0.80..0.95), 只在白底段
    staff=np.zeros_like(hi)
    for k in range(5):
        ff=0.80+k*0.035; staff|=hi&(np.abs(f-ff)<0.0045)
    staff&=(idx==NAMES.index('white'))&(((t>0.14)&(t<0.37))|((t>0.63)&(t<0.86)))&(np.abs(nrm[...,0])>0.75)
    idx[staff]=NAMES.index('gold')
    # 音符(实心椭圆+符干) 沿谱线
    for k,tn in enumerate(list(np.linspace(0.17,0.34,4))+list(np.linspace(0.66,0.83,4))):
        zc=zmin+tn*(zmax-zmin); fc=0.80+((k*3)%5)*0.035
        d=((z-zc)/0.09)**2+((f-fc)/0.028)**2
        note=hi&(d<1)
        stem=hi&(np.abs(z-(zc+0.08))<0.012)&(f>fc)&(f<fc+0.12)
        idx[(note|stem)&(idx!=NAMES.index('peri'))]=NAMES.index('gold')
    # 甲板以上的船体材质件(舰桥基座等): 上层白 + 层线
    upper=S&(y>ez+0.06)
    idx[upper]=NAMES.index('dhwhite')
    for yy in np.arange(0.78,2.0,0.32): idx[upper&(np.abs(y-yy)<0.01)]=NAMES.index('peri_d')
    # ---- 甲板 ----
    U=up
    idx[U]=NAMES.index('deck')
    r=ax/hwz
    for k in range(3):  # 5 线在透视下读成木板 → 3 线疏排
        rr=0.76+k*0.065; idx[U&(np.abs(r-rr)<0.0065)&(r<1.2)]=NAMES.index('gold')
    idx[U&(np.abs(r-0.725)<0.006)]=NAMES.index('glow')   # P3: 金线组内侧天青发光线
    idx[U&(np.abs(r-0.94)<0.006)]=NAMES.index('glow')    # b3: 甲板边内侧第二条
    # 中线走道 periwinkle 细带
    idx[U&(ax<0.05)]=NAMES.index('peri_d')
    # 徽记: 艏甲板(主炮前) + 艉甲板
    em=emblem()
    for zc,s,rot in [(-6.3,1.9,0),(6.9,1.5,1)]:
        u=(x/s)+0.5; v=((z-zc)/s)+0.5
        if rot: v=1-v
        dv=sample_decal(em,u,v)
        idx[U&(dv==1)]=NAMES.index('peri'); idx[U&(dv==2)]=NAMES.index('gold')   # P5: 甲板已冷白 → 徽记填 periwinkle
    # UV 复用的平铺区(多次覆盖且朝上) → 纯甲板色, 避免图案错乱
    idx[up&(cov>1)]=NAMES.index('deck')
    if upc is not None: idx[covd&(upc>0)&(cov>upc)]=NAMES.index('deck')  # 甲板/舷侧共用纹素 → 纯甲板色
    return idx

def paint_hull_blocks(pos,nrm,cov,upc=None):
    """P3 船体「舞台光束」大块硬边分区(知更鸟语言; 学标杆原则: 大块/硬边/分区/发光走结构, 图案不抄):
    艏、艉两道 periwinkle 光束楔(直线硬边)向舯部翼组处汇聚并抬高, 中间留白「聚光」缝 + 金色三线谱折角;
    楔内下层 royal 窄楔; 楔顶边内侧天青发光线, 水线界一条发光; 艏艉 navy 尖楔+星点。甲板: 两侧大 periwinkle 梯形板 + 内沿发光。"""
    H,W=cov.shape; idx=np.full((H,W),NAMES.index('white'),np.uint8)
    x,y,z=pos[...,0],pos[...,1],pos[...,2]; ax=np.abs(x); covd=cov>0
    up=covd&((nrm[...,1]>0.65)|((pos[...,1]>0.2)&(nrm[...,1]<-0.2)&(np.abs(nrm[...,0])<0.6)))
    if upc is not None: up|=covd&(upc>0)
    side=covd&~up
    zmin,zmax=-8.864,8.80; t=(z-zmin)/(zmax-zmin)
    bins=np.linspace(zmin,zmax,90); e=np.zeros(len(bins)-1); hw=np.zeros(len(bins)-1)
    sz=z[side]; sy=y[side]; sax=ax[side]
    for i in range(len(bins)-1):
        m=(sz>=bins[i])&(sz<bins[i+1])
        if m.any():
            mm=m&(sax>np.percentile(sax[m],60)); e[i]=np.percentile(sy[mm],97) if mm.any() else 0.45; hw[i]=sax[m].max()
        else: e[i]=0.45; hw[i]=0.3
    ez=np.interp(z,(bins[:-1]+bins[1:])/2,np.clip(e,0.35,0.56)); hwz=np.interp(z,(bins[:-1]+bins[1:])/2,np.maximum(hw,0.15))
    f=np.clip(y/ez,-2,2); S=side
    TC=0.54   # 翼组站位(游戏 z≈−0.7)
    # A3 提亮: 深藏青只留水线下靴带(f<0.12), 0.12–0.22 periwinkle-深; 光束底改乳白, 光束本身 periwinkle 为主
    # P5.1: 深蓝靴收成细带(仍饱和 #243A78, 不发灰闷); 其上 royal 细水线; 白底从 0.12 起(抬白但不砍 peri 饱和)
    idx[S&(f<0.08)]=NAMES.index('navy'); idx[S&(f>=0.08)&(f<0.12)]=NAMES.index('royal')
    hi=S&(f>=0.12)
    idx[hi]=NAMES.index('white')
    # A4: 斜条合并 → 每半船 2 块大硬边光束(大 periwinkle + 次 peri_d), 端部 royal 楔; 一条连续发光主线贯穿全船
    LEAN=0.030
    def beam(t0,t1,col,sgn,edge_col=None):
        tt=t-sgn*LEAN*(f-0.22)
        m=hi&(tt>t0)&(tt<t1); idx[m]=NAMES.index(col)
        if edge_col:
            e=hi&(np.abs(tt-(t1 if sgn>0 else t0))<0.0035); idx[e]=NAMES.index(edge_col)
        return m
    # A5: 每侧 4 块(艏 royal 楔 / 前大 periwinkle / 后大 periwinkle / 艉 royal 楔), 其余留乳白大底
    # P5.1: 保持 P5 peri 光束宽度(鲜明点缀), royal 小楔
    beam(0.05,0.085,'royal',1);  beam(0.26,0.42,'peri',1,'glow')
    beam(0.64,0.78,'peri',-1,'glow');  beam(0.925,0.955,'royal',-1)
    # 舯「聚光」区: 乳白 + 金色三线谱折角
    gap=hi&(np.abs(t-TC)<0.045); idx[gap]=NAMES.index('white')
    for k in range(3):
        c=TC+(k-1)*0.012
        v=hi&(np.abs((t-c)-0.035*(f-0.55))<0.0035)&(f>0.30)&(f<0.92)
        idx[v]=NAMES.index('gold')
    # 发光主线: 沿舷 f≈0.34 贯穿艏艉, 在舯部聚光区折成向上的 V(指向翼根), 线宽加粗
    fl=0.34+0.50*np.clip(1-np.abs(t-TC)/0.045,0,1)
    idx[S&(np.abs(f-fl)<0.048)&(t>0.03)&(t<0.97)]=NAMES.index('glow')   # P5.1: 主线略加粗(0.040→0.048, 勿到 P6 过粗)
    # 艏艉 navy 尖楔 + 星点
    bow=hi&(t<0.015+0.028*(1-f)); stern=hi&(t>0.985-0.022*(1-f))   # P5.1: 艏艉 navy 楔略收
    idx[bow|stern]=NAMES.index('navy')
    st=stars_field(z,y)&(bow|stern|(S&(f<0.22)&(y>-0.25)))
    idx[st]=NAMES.index('star')
    upper=S&(y>ez+0.06); idx[upper]=NAMES.index('dhwhite')
    # 甲板
    U=up; idx[U]=NAMES.index('roof'); r=ax/hwz   # P5: 甲板底冷白, 两侧大板 periwinkle(清透蓝)   # A3: 甲板底由深紫改 periwinkle, 两侧大板乳紫(roof)
    pA=U&(r>0.62)&(r<0.93)&(((t>0.13)&(t<TC-0.06))|((t>TC+0.06)&(t<0.88)))
    # 板端斜切
    pA&=~((t<0.13+0.06)&(r<0.62+(0.13+0.06-t)*5))&~((t>0.88-0.06)&(r<0.62+(t-(0.88-0.06))*5))
    idx[pA]=NAMES.index('peri')
    idx[pA&(r<0.66)]=NAMES.index('glow')
    idx[U&(ax<0.05)]=NAMES.index('peri_d')
    em=emblem()
    for zc,s_,rot in [(-6.3,1.9,0),(6.9,1.5,1)]:
        u=(x/s_)+0.5; v=((z-zc)/s_)+0.5
        if rot: v=1-v
        dv=sample_decal(em,u,v)
        idx[U&(dv==1)]=NAMES.index('peri'); idx[U&(dv==2)]=NAMES.index('gold')   # P5: 甲板已冷白 → 徽记填 periwinkle
    idx[up&(cov>1)]=NAMES.index('deck')
    if upc is not None: idx[covd&(upc>0)&(cov>upc)]=NAMES.index('deck')
    return idx

def paint_dh(pos,nrm,cov):
    H,W=cov.shape; idx=np.full((H,W),NAMES.index('dhwhite'),np.uint8)
    x,y,z=pos[...,0],pos[...,1],pos[...,2]; covd=cov>0
    up=covd&(nrm[...,1]>0.7); side=covd&~up
    # 下层(裙): y<0.78 periwinkle, 上缘金线
    idx[side&(y<0.78)]=NAMES.index('dhwhite'); idx[side&(y<0.62)]=NAMES.index('peri')   # P5.1: 回 P5 裙段
    idx[side&(np.abs(y-0.78)<0.012)]=NAMES.index('gold')
    idx[side&(y>0.80)&(y<0.818)]=NAMES.index('glow')   # P3: 裙线上方发光线
    # 层间细带: 每 0.32 一条 periwinkle 细线(贴层高)
    if STYLE=='blocks':   # P3: 细层线 → 一道宽 periwinkle 斜切带(舰桥腰)+上下发光沿
        band=side&(y>1.30+0.10*np.tanh(z))&(y<1.62+0.10*np.tanh(z))
        idx[band]=NAMES.index('peri')
        idx[side&(np.abs(y-(1.30+0.10*np.tanh(z)))<0.012)]=NAMES.index('glow')
        idx[side&(np.abs(y-(1.62+0.10*np.tanh(z)))<0.012)]=NAMES.index('glow')
    else:
      for i,yy in enumerate(np.arange(1.10,2.9,0.32)):
        idx[side&(y>yy-0.045)&(y<yy)]=NAMES.index('peri' if i%2==0 else 'peri_d')
        idx[side&(np.abs(y-yy)<0.008)]=NAMES.index('gold')
    if STYLE=='blocks':   # A4: 发光主线上舷楼 —— 翼组站位(烘焙 z≈+0.7)从裙线竖直接到翼根高度
        idx[side&(np.abs(z-0.70)<0.050)&(y>0.80)&(y<2.05)]=NAMES.index('glow')
    # 烟囱: 高处(y>1.55)且 z 在烟囱段 → 深蓝顶 + 紫身 + 金环
    fun=side&(z>0.4)&(z<2.6)&(y>1.45)
    idx[fun]=NAMES.index('violet'); idx[fun&(y>2.05)]=NAMES.index('navy')
    idx[fun&(np.abs(y-2.02)<0.02)]=NAMES.index('gold')
    idx[fun&(y>1.975)&(y<1.99)]=NAMES.index('glow')   # P3: 烟囱金环下发光环
    # 顶面: 淡紫白; 但艏艉/舯部 y<0.6 的 DeckHouse 顶面其实是主甲板铺板 → 甲板色(否则甲板上出现白楔)
    idx[up]=NAMES.index('roof')
    idx[up&(y<0.6)]=NAMES.index('roof' if STYLE=='blocks' else 'deck')   # P5: 与甲板冷白一致
    # P3 锚: 艏锚是 DeckHouse 材质(Hull_Bow 子网格, UV≈u0.28-0.48 v0.58-0.68), 原先随裙色成 periwinkle/白糊 → 金锚
    idx[covd&(z<-8.1)&(y>0.33)&(y<0.62)&(np.abs(x)>0.08)]=NAMES.index('gold')
    return idx

def paint_turret(pos,nrm,cov,kind):
    H,W=cov.shape; idx=np.full((H,W),NAMES.index('turret'),np.uint8)
    x,y,z=pos[...,0],pos[...,1],pos[...,2]; covd=cov>0
    up=covd&(nrm[...,1]>0.7); side=covd&~up
    ymax=np.percentile(y[covd],99); zmin=z[covd].min(); zmax=z[covd].max(); xmax=np.abs(x[covd]).max()
    if STYLE=='blocks':   # T1: 配合装甲围板 —— 侧面白为主(上部露出段), 底座圈 periwinkle; 去掉顶缘金线/紫座圈(边沿杂条纹)
        idx[side]=NAMES.index('turret')
        idx[side&(y<0.035*(ymax/0.24))]=NAMES.index('peri')
    else:
        # 侧面 periwinkle, 底部紫色座圈, 顶缘金线
        idx[side]=NAMES.index('peri')
        idx[side&(y<0.035*(ymax/0.24))]=NAMES.index('violet')
        idx[side&(y>ymax*0.86)]=NAMES.index('gold')
    # 顶面: 白 + 中线 periwinkle 缎带 + 两侧金细线 + 尾部八芒星
    idx[up]=NAMES.index('turret')
    w=0.12*xmax
    if STYLE=='blocks':   # T1: 顶面中线 royal 宽分区(硬边), 不要两侧金细线
        idx[up&(np.abs(x)<w*1.25)]=NAMES.index('peri')   # P5: royal → periwinkle(减深色)
    else:
        idx[up&(np.abs(x)<w)]=NAMES.index('peri')
        idx[up&(np.abs(np.abs(x)-w*1.35)<0.012*xmax/0.5)]=NAMES.index('gold')
    em=emblem(512)
    s_=xmax*0.9; zc=zmin+0.28*(zmax-zmin)
    dv=sample_decal(em,x/s_+0.5,(z-zc)/s_+0.5)
    idx[up&(dv==2)]=NAMES.index('gold'); idx[up&(dv==1)&(np.abs(x)>=w)]=NAMES.index('peri_d')
    # 原图深色炮衣/炮口罩(暗八边形) → 深蓝
    return idx

def compose(idx,orig,mode,k=0.55,lo=0.72,hi=1.22):
    """色号→贴图色, 乘原图细节比(保留板缝/AO)。"""
    cols=np.stack([hx(TEXOVR[n]) if n in TEXOVR else to_tex(hx(PAL[n]),COLMODE.get(n,mode) if mode!='raw' else 'raw') for n in NAMES])
    base=cols[idx]
    # 抗锯齿: 色号图是最近邻放大的, 港口近景弧线/星点出现台阶 → 底色轻微高斯(1px)
    base=np.stack([np.asarray(Image.fromarray(base[...,c].astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.0)),float) for c in range(3)],-1)
    lum=np.asarray(orig.convert('L'),float)+1
    blur=np.asarray(orig.convert('L').filter(ImageFilter.GaussianBlur(24)),float)+1
    ratio=np.clip(lum/blur,lo,hi)**k
    out=np.clip(base*ratio[...,None],0,255).astype(np.uint8)
    return out

def upsample_idx(idx,size):
    return np.asarray(Image.fromarray(idx).resize((size,size),Image.NEAREST))

def dilate_fill(idx,cov,fill,far=None):
    """未覆盖像素: 向外扩 8px 防 mip 漏边; 更远的未烘焙纹素(如炮管等未计入实例的部件)填 far"""
    m=Image.fromarray(((cov>0)*255).astype(np.uint8))
    if far is not None:
        g=m
        for _ in range(4): g=g.filter(ImageFilter.MaxFilter(5))
        idx=idx.copy(); idx[np.asarray(g)==0]=far
    im=Image.fromarray(idx); res=idx.copy()
    for _ in range(4):
        grown=np.asarray(im.filter(ImageFilter.MaxFilter(5)))
        hole=(np.asarray(m)==0)
        res[hole]=grown[hole]; im=Image.fromarray(res)
    return res

def side_preview(pos,nrm,cov,idx,out,mode):
    cols=np.stack([to_tex(hx(PAL[n]),'raw') for n in NAMES]).astype(np.uint8)
    m=(cov>0)&(pos[...,0]>0.0)&(np.abs(nrm[...,0])>0.3)
    z=pos[...,2][m]; y=pos[...,1][m]; c=cols[idx[m]]
    Wp,Hp=1800,320; img=np.full((Hp,Wp,3),30,np.uint8)
    px=((z+9)/18*Wp).astype(int).clip(0,Wp-1); py=(Hp-1-((y+0.7)/3.0*Hp)).astype(int).clip(0,Hp-1)
    img[py,px]=c
    top=np.full((420,1800,3),30,np.uint8)
    mu=(cov>0)&(nrm[...,1]>0.65)
    z2=pos[...,2][mu]; x2=pos[...,0][mu]; c2=cols[idx[mu]]
    qx=((z2+9)/18*1800).astype(int).clip(0,1799); qy=((x2+1.3)/2.6*420).astype(int).clip(0,419)
    top[qy,qx]=c2
    Image.fromarray(np.vstack([img,top])).save(out)

if __name__=='__main__':
    ap=argparse.ArgumentParser(); ap.add_argument('--hull-pos'); ap.add_argument('--dh-pos'); ap.add_argument('--hull-a'); ap.add_argument('--dh-a')
    for k in ('fgm','fgs036','fgs053'): ap.add_argument(f'--{k}-pos'); ap.add_argument(f'--{k}-a')
    ap.add_argument('--style',default='stripes',choices=['stripes','blocks']); ap.add_argument('--mode',default='port',choices=['port','raw','half']); ap.add_argument('--outdir',required=True); ap.add_argument('--size',type=int,default=4096)
    a=ap.parse_args(); stats={}; STYLE=a.style
    if a.style=='blocks': paint_hull=paint_hull_blocks
    for key,posf,af,fn in [('hull',a.hull_pos,a.hull_a,paint_hull),('dh',a.dh_pos,a.dh_a,paint_dh)]:
        d=np.load(posf); pos,nrm,cov=d['pos'],d['nrm'],d['cov']
        idx=fn(pos,nrm,cov,d['upc']) if (key=='hull' and 'upc' in d.files) else fn(pos,nrm,cov)
        side_preview(pos,nrm,cov,idx,f'{a.outdir}/{key}_preview_world.png',a.mode)
        fill=NAMES.index('white' if key=='hull' else 'dhwhite')
        idx2=dilate_fill(idx,cov,fill)
        Image.fromarray(idx2).save(f'{a.outdir}/{key}_idx.png')
        Image.fromarray(((idx2==NAMES.index('glow'))*255).astype(np.uint8)).save(f'{a.outdir}/{key}_glow.png')   # P3 发光遮罩(wowsmod glow --mask)
        if key=='hull':   # P5: 舷侧遮罩(给 tex channel --mask 只压舷侧法线, 保留甲板木纹)
            sm=(cov>0)&(np.abs(nrm[...,0])>0.35)&(nrm[...,1]<0.65)
            Image.fromarray((sm*255).astype(np.uint8)).save(f'{a.outdir}/hull_side_mask.png')
        big=upsample_idx(idx2,a.size)
        orig=Image.open(af).convert('RGB')
        out=compose(big,orig,a.mode)
        Image.fromarray(out).save(f'{a.outdir}/{key}_a_p2_{a.mode}.png')
        u,cn=np.unique(idx[cov>0],return_counts=True)
        stats[key]={NAMES[i]:round(float(c)/cn.sum(),3) for i,c in zip(u,cn)}
    for key,posf,af in [('FGM049',a.fgm_pos,a.fgm_a),('FGS036',a.fgs036_pos,a.fgs036_a),('FGS053',a.fgs053_pos,a.fgs053_a)]:
        if not posf: continue
        d=np.load(posf); pos,nrm,cov=d['pos'],d['nrm'],d['cov']
        orig=Image.open(af).convert('RGB'); sz=orig.size[0]
        idx=paint_turret(pos,nrm,cov,key)
        # 原图暗块(炮衣)保持深蓝
        lum=np.asarray(orig.convert('L').resize((cov.shape[1],cov.shape[0])),float)
        idx[(cov>0)&(lum<55)]=NAMES.index('navy')
        if key=='FGM049':
            # 炮管 UV 岛(圆柱展开矩形, 底缘为炮口熏黑渐变): 蒙皮顶点在 glb 绑定姿态, 世界坐标不可用 → 按 UV 矩形
            H,W=cov.shape; vv,uu=np.mgrid[0:H,0:W]/np.array([H,W])[:,None,None]
            br=(uu>0.585)&(vv>0.142)&(vv<0.488)
            idx[br]=NAMES.index('navy')
            idx[br&(np.abs(vv-0.462)<0.006)]=NAMES.index('gold')   # 炮口金环
            idx[br&(np.abs(vv-0.20)<0.004)]=NAMES.index('gold')    # 根部金环
        idx2=dilate_fill(idx,cov,NAMES.index('turret'),far=NAMES.index('navy'))  # 炮管不在烘焙实例里 → 深蓝
        big=np.asarray(Image.fromarray(idx2).resize((sz,sz),Image.NEAREST))
        Image.fromarray(compose(big,orig,a.mode)).save(f'{a.outdir}/{key}_a_p2_{a.mode}.png')
        u,cn=np.unique(idx[cov>0],return_counts=True); stats[key]={NAMES[i]:round(float(c)/cn.sum(),3) for i,c in zip(u,cn)}
    json.dump(dict(stats=stats,pal=PAL,mode=a.mode),open(f'{a.outdir}/p2_paint_stats.json','w'),ensure_ascii=False,indent=1)
    print(json.dumps(stats,ensure_ascii=False))
