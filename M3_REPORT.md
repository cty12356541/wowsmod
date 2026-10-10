# M3 报告:模型写回

> 状态:已完成验收(2026-10-10)。“不确定”的地方明确标注。

## 进度

- [x] 装配器移植到 Rust(`wowsmod-formats::assemble`,CLI `wowsmod assemble`),与 Python 版逐项一致(勃艮第、黎塞留)
- [x] assets.bin 交叉核对挂点矩阵(勃艮第 115/115、黎塞留 154/154 逐位一致)
- [x] GLB→.geometry 编码器(`geomedit`)+ visual 文本编辑(`visual`)+ 层级 GLB 读写(`scene`)
- [x] `wowsmod model list/scale/move/rotate/replace/remove/add/rebuild/export-blender/import-blender/undo/replay`(box 上自测通过)
- [x] Windows 构建 + 全船 geometry 字节一致往返(21/21)+ 等价重建自检(20/20 等价)
- [x] 港口实测:等价重建 / 缩放(挂点+部件) / 移动 / 替换 / 删除 / 新网格 / Blender 往返,7 次进港全部无报错无崩溃
- [x] 对比图(box `/workspace/m3/ingame/`)、DESIGN.md 第 10 节 / PLAN.md 勾选

## 1. 装配器(assemble)

`wowsmod assemble <船> -o ship.glb [--table t.json] (--sdk <SDK船目录> | --project <mod项目>) [--geom-dir D]... [--gp gp.json] [--hullsets intact|broken|main|all] [--kinds Artillery,ATBA|all] [--full]`

- 船名可写 `PFSB510_Bourgogne` / `FSB510_Bourgogne` / `PFSB510`;不给 `--gp` 时从游戏读 `GameParams_py3.data`。
- geometry 来源顺序:项目/SDK 目录里的非占位文件 → `--geom-dir`(按文件名递归) → 游戏包。
- `--project` 用项目里(可能已编辑的)visual/geometry 装配,可在进游戏前预览改动。
- `--full` 额外输出法线/UV/材质分组(Python 版只有 POSITION)。

与 Python 参考输出(`tools/assembler`)对比(`scripts/m3/compare_assembly.py`):

| 船 | 节点 | 节点矩阵最大差 | 顶点位置/索引 | 挂点 14/11 个的矩阵、glTF 位置/偏航、炮口方向、型号/分段/炮管数 | 分段统计 |
|---|---|---|---|---|---|
| 勃艮第 | 18=18 | 0.0 | 全部一致 | 全部一致(差 0) | 一致 |
| 黎塞留 | 15=15 | 0.0 | 全部一致 | 全部一致(差 0) | 一致 |

## 2. assets.bin 交叉核对挂点矩阵

`scripts/m3/hp_assets_check.py <assets.bin> <SDK船目录> out.json`:取 SDK `*_ports.visual` 里每个 `HP_*` 节点的局部矩阵(16 个 float32),在 assets.bin(游戏实际加载的原型库,26.10)里按 float32 搜索。

- 勃艮第 115 个挂点、黎塞留 154 个挂点:**全部找到完全相同的 16 个 float(最大差 0.0)**,均位于 assets.bin 约 91.5–93.4 MB 的同一区域(推测为 VisualPrototype 骨架矩阵数组)。
- 结论:SDK 明文 visual 的挂点矩阵 = 游戏原型库数据。局限:按数值匹配,没有按结构解析出“哪个原型的哪个节点”(VisualPrototype 节点表结构 **不确定**,未解析)。


## 3. geometry 编码器(GLB → .geometry)

模块 `wowsmod-formats::geomedit`(读写)+ `visual`(.visual 文本编辑)+ `scene`(层级 GLB)。

- **读→原样写**:勃艮第全部 21 个 geometry(`geom out\m3\bour_game --roundtrip`)**21/21 逐字节一致**(重排布局一致 + 重新编码一致),报告 `out\m3\roundtrip_bour.json`。
- **映射名 / id**:mapping id = murmur3(名字);名字规则 `X`→`XShape`,已以 `Shape<数字>` 结尾的(如 `BD018_Ster_Sell_Sight_lodShape2`)原样使用。这一条是本轮在 FD038 上发现并修正的。
- **顶点格式**:新网格取模板渲染集的顶点格式(`--like` 正则,或 GLB 材质名 = 渲染集名)。法线/切线按原格式打包(4 字节有符号压缩),UV 为 half/float,和模板一致。逐顶点模板:位置完全相同的取原顶点,否则取最近的原顶点(用来补骨骼索引/权重/附加字节)。
- **bbox / texelDensity**:geometry 侧重新计算;.visual 的渲染集与分段 bbox 取并集;节点矩阵同步。texelDensity 沿用模板渲染集的值(引擎用途 **不确定**,实测无可见影响)。
- **装甲 / 碰撞**:勃艮第有 2 个文件带 armor、7 个带 collision。编码器**原样保留**这两块(等价重建 20/20 `armor_equal`、`collision_equal`)。**影响**:缩放、移动、替换、新增只改渲染网格,**不改装甲和碰撞**。外形变了,命中判定仍按原来的装甲/碰撞体计算。只在港口看没有影响;进战斗后的判定是否“看起来不对”,按规则没有进战斗测,**不确定**。
- **等价重建**(`model rebuild`,所有网格按 GLB 路径解出再编码):20 个单元全部等价。540,735 顶点、395,648 三角:位置/UV 逐位相同,法线差 0,其他字节差 0,三角形 0 差(2 个只是起点轮换)。文件总大小 9,118,652→9,118,810 B。
  - `--recompute-tangents` 变体(rebuildt):切线重算后部分顶点切线字节最大差 255(符号翻转)。没有进港测,港口效果 **不确定**,所以默认保留原切线。
- **骨骼 / 蒙皮**:船体渲染集大多是节点局部空间。MidBack 的 3 个烟囱渲染集是蒙皮世界空间(Root/Chimney 骨骼)。变换时:世界空间渲染集顶点乘 T,其余乘 W·T·W'⁻¹。骨骼只吸收平移和旋转,不吸收缩放(缩放写进顶点),所以蒙皮权重不用改。炮塔部件的旋转骨骼(炮塔/炮管节点)按同样方式移动,港口里炮塔外形正确;战斗中炮塔转动和俯仰是否正常,**未测(不进战斗)**。
- **LOD 策略**:
  - 缩放、移动、删除、新增:对同一 geometry 里**所有 LOD 族**同样处理,新网格加到所有 LOD visual;
  - Blender 往返:改动族的 lod1–3 渲染集改指向 LOD0 网格(远处三角多一些,但不会切回原形)。
  - 港口拉到最远(x16)时,所有改动都还在,没有看到弹回原形(`m3_lod_far.png`)。港口相机最远距离有限,是否已切到最低一级 LOD **不确定**。
  - `lod4`(Full)没有任何编辑会更新它,用途 **不确定**,港口无可见影响。

## 4. model 命令

`wowsmod [--json] model <list|scale|move|rotate|replace|remove|add|rebuild|export-blender|import-blender|undo|replay> --project P [--part X | --hp HP_xxx | --section S --renderset R] ...`

- 每次编辑写进 `edits/NNN_model_*.json`,内容包括:参数(可重放)、改前/改后 sha256、备份目录 `edits/.undo/<id>/`;写入是原子的,一次性提交。
- `undo` 只有在 sha 一致时才还原(`--force` 可强制),还原后记录改名为 `.json.undone`。
- `replay --from 另一项目` 重放全部编辑。combo 项目就是由 scalehp+move+replace+newmesh 四个项目 replay 生成的,文件逐一一致。
- 挂点(HP_xxx)缩放和旋转以挂点为中心,移动在分段空间内平移。挂点不能 replace(装什么型号由 GameParams 决定),`remove --hp` 的做法是缩到 0.001。
- `replace --with 其它部件 [--with-sdk 目录]`:复制 geometry,visual 按 LOD 一一对应(来源 LOD 少时用来源的最后一级)。`replace --glb`:用用户 GLB 替换。

## 5. 港口实测(勃艮第,2026-10-10 约 15:55–16:32 北京时间)

流程(`scripts/ingame/m3_game.ps1` 的 Run-Session / End-Session):核对 res_mods → `install --dev` → 启动器(矩形 257,110,2049,1230 核对后点 ИГРАТЬ) → 等 100 s 进港 → 固定机位截图(v1 船尾斜视, v2 侧视, v3 拉远, v4 舯部拉近, x14 船尾斜后, x15 后主炮特写, x16 最远) → 取本次 python.log → Alt+F4,截图确认“退出游戏?”对话框(按钮像素校验)后点“是”(1185,712) → `uninstall` → 核对 res_mods 只剩 readme.txt 且 SHA-256 = 0157BF…2F3F。

只在港口,没进战斗,没花费,没领奖励;7 次会话结束后 res_mods 都已复原。

| 会话 | 内容 | registerShipMod | Replace 条数 | 意外报错 | 崩溃 | 结果 |
|---|---|---|---|---|---|---|
| base(×2) | SDK 原样导入(改前基准) | 成功 | 26 | 无 | 无 | 基准 |
| rebuild | 20 个 geometry 全部经编码器等价重建 | 成功 | 26 | 无 | 无 | 肉眼无差别;船体区域像素差 ≤ 两次 base 之间的差(水面/光照动态) |
| combo | HP_FGM_3 ×1.5 + HP_FGS_8/9 上移 0.25 + FGS036→FGS053 + 烟囱顶盖新网格 | 成功 | 26 | 无 | 无 | 4 项均可见 |
| move | HP_FGS_8/9 上移 0.25 | 成功 | 26 | 无 | 无 | 可见 |
| scalepart | 部件 FGM049 几何 ×1.5(三座主炮) | 成功 | 26 | 无 | 无 | 可见 |
| remove | 删除 MidBack 渲染集 Chimney(全部 LOD) | 成功 | 26 | 无 | 无 | 烟囱消失 |
| blender | Blender 5.2 往返:3 个烟囱对象竖向 ×1.35 | 成功 | 26 | 无 | 无 | 烟囱变高 |

“意外报错”指排除已知、和模组无关的报错之后剩下的:paths.xml、IME、`ModsSDK.zip` 无主脚本(M2 已知)、RewardSourceSystem、XmppChat。M3 没有出现新的报错。崩溃检查:`D:\Korabli` 下近 1 小时没有 dmp/crash 文件。

像素差(船体区域平均灰度差 / 差>30 的像素比例):

| 比较 | 侧视 v2 | 近景 v4 |
|---|---|---|
| base 第 1 次 vs base 第 2 次 | 8.51 / 0.63% | 7.51 / 5.0% |
| base 第 2 次 vs rebuild | 6.35 / 0.53% | 5.15 / 1.34% |

结论:等价重建与原版的差,小于原版两次进港之间的自然差。

### 对比图(box `/workspace/m3/ingame/`,原始截图在 `raw/`;Windows 原件 `out\m3_ingame\<项目>\`,2560×1440 全尺寸 + `_small` 1280×720)

- 等价重建:`m3_cmp_rebuild_equivalent.png`(侧视)、`m3_cmp_rebuild_equivalent_near.png`(近景)
- 缩放:`m3_cmp_scale_hp.png`(挂点 HP_FGM_3 后主炮 ×1.5)、`m3_cmp_scale_part.png`(部件几何 FGM049 ×1.5;港口时间变成黄昏,所以光照不同)
- 移动:`m3_cmp_move.png`
- 替换:`m3_cmp_replace.png`(127 双联 → 152 三联外形)
- 删除:`m3_cmp_remove.png`
- 新网格:`m3_cmp_newmesh.png`(260 顶点顶盖 + 桅杆;桅杆清晰可见,顶盖与原格栅顶颜色接近,不太显眼)
- Blender 往返:`m3_cmp_blender.png`
- 远景/LOD:`m3_lod_far.png`

## 6. 局限与不确定

- **挂点矩阵缩放**:引擎采用了(后主炮确实变成 1.5 倍)。战斗中炮塔转动、射界、炮口火焰位置是否随之变化,**不确定(未进战斗)**。
- **替换炮管数不一致**:FGS036(2 管)→FGS053(3 管)外形,GameParams 仍是 2 管。港口显示正常;战斗中开火特效挂点(HP_gunFire)对不上会怎样,**不确定**。
- **装甲/碰撞不随外形变**(见第 3 节)。
- **相机不完全可复现**:同一套鼠标序列,不同会话之间有约 20 px 的取景偏差。拼图时已手动对齐水平偏移,remove/newmesh 的偏移为 −21 px。
- **lod4 / texelDensity 的用途**:**不确定**。
- **切线重算(rebuildt)的港口效果**:**不确定(未进港测)**;默认路径保留原切线,不受影响。
