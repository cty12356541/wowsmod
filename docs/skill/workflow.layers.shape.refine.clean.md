# workflow.layers.shape.refine.clean: 原壳清理/降噪
上级: workflow.layers.shape.refine
下级: 无
---
**何时**: 新涂装/发光主线做完后, 港口近景看到原船壳上的小突起(舷窗盖、通风口、小舱口、锚链/线缆、小管件)把色块和发光线打碎; 或用户说「原壳细节太杂」。
**目标**: 不改设计本身, 只让色块与发光主线连续; 挂点/炮塔/LOD 不受影响。
**输入**: 项目(贴图层已定)、无外带件的整船 `geometry.glb`(工作台缓存; 别用已带外带件的项目缓存)、世界坐标烘焙 `*_pos.npz` 与画师色号图 `*_idx.png` / `*_glow.png`。
**盘点**(只读):
1. `wowsmod model prune --project <P> --renderset '^(Bow|MidFront|MidBack|Stern)$' --facing side|up --max-size 0.08 --dry-run` → 每个 LOD 渲染集的游离小连通块数/三角数, LOD0 全部列出(中心/对角线/三角数)。
2. 排序: `scripts/model/clutter_rank.py <pos.npz> <idx.png> <glow.png> <dry-run.json>...` → 每块包围盒内贴图像素的色号种类数与是否含发光; `texels=0` = 该块 UV 不在世界坐标涂装覆盖内(颜色与所在色块无关, 必然打碎色块) → 最优先。
3. 统计用 `scripts/model/clutter_inventory.py`(Blender 读 glb, 按尺寸/舷侧/朝上分类)。
**删/压/融判断**:
- 删(`model prune`): 游离(不与船壳共顶点)、对角线 < 0.08(约 1 m 内)、在舷侧色块面上(`--facing side`)或主甲板(`--facing up --box=…,y<0.75`)。只退化三角形, 顶点缓冲/节点/挂点不变; 各 LOD 按同条件独立判定。
- 不删: 与船壳焊接的(连通块大)、上层建筑上的功能件(测距仪/指挥仪/炮)、栏杆网(Net/wire 渲染集)、任何挂点附近会穿帮成空洞的件。
- 融(贴图): 删不了、又在发光线/色块边界上的 → 在 `_a` 上按周围色块色覆盖该块 UV, 必要时 `_ao` 局部提亮(`tex channel --mul`), 让它「读成色块的一部分」。
**命令**: `wowsmod model prune --project <P> --renderset <正则> [--part|--section] --max-size <d> [--box x0,y0,z0,x1,y1,z1 ...] --facing side|up|any [--side-band 0.06] [--dry-run]`(通用, 不写死船名)。
**验收**: ① dry-run 与实际删除数一致、LOD0–3 都有处理(LOD2/3 少是正常); ② 工作台同机位前后图(对比时两边都用同类 step: 改模型后预览会切「全模型」显示, 测距仪等深色件会变浅, 不是 prune 造成的); ③ 港口白天同角前后近景, 色块/发光线更连续, 无空洞/漏光; ④ `turret_check`/挂点不受影响。
**完成标准**: 盘点表(按打碎程度排序) + 删除清单 + 港口前后对比图写进设计书; 残余不能删的项写明为何保留。
**融(r35/P5)**: 舷窗圆点/流锈主要在 `_a` 明暗(重涂保留原图亮度) → `despeckle.py <a> <out> --mask hull_side_mask.png --thr 7`(只换比周围暗、同色相、饱和底上的小斑; 发光/金/白底铭牌不动); 遮罩由 paint 按法线产出, 不写死 UV 行带。凹凸在 `_n` → `tex channel Hull_n --channel r/g --set 128 --mask hull_side_mask.png` 只压舷侧, 甲板木纹保留。apply 参数 `Despeckle=1 FlatN=1`(流水线内执行)。验收: 港口同角前后, 舷窗消失。
