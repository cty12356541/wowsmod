# M3 报告:模型写回(进行中,断点续作用)

> 状态:进行中。本文件随进度更新;“不确定”的地方明确标注。

## 进度

- [x] 装配器移植到 Rust(`wowsmod-formats::assemble`,CLI `wowsmod assemble`),与 Python 版逐项一致(勃艮第、黎塞留)
- [x] assets.bin 交叉核对挂点矩阵(勃艮第 115/115、黎塞留 154/154 逐位一致)
- [x] GLB→.geometry 编码器(`geomedit`)+ visual 文本编辑(`visual`)+ 层级 GLB 读写(`scene`)
- [x] `wowsmod model list/scale/move/rotate/replace/remove/add/rebuild/export-blender/import-blender/undo/replay`(box 上自测通过)
- [ ] Windows 构建 + 全船 geometry 字节一致往返 + 等价重建自检
- [ ] 港口实测:等价重建 / 缩放 / 移动 / 替换 / 删除 / 新网格 / Blender 往返
- [ ] 对比图、DESIGN.md / PLAN.md 更新

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

