# ref.model.import: 导入外部模型
上级: ref.model
下级: 无
---
**主题模式**: 只 import 道具部件(PMX/GLB 去掉人体材质后导出), 禁止全身/半身雕像。复刻参考视频确有人物模型时须用户确认。
人物站姿(仅复刻/用户明确要求): PMX 常 A/T 字, `pmx2glb … --pose arms-down[:度]` 再 import。

`wowsmod model import --project <项目> --mesh <glb|obj> --like <模板渲染集> [--hp HP_x | --part <部件> | --section <分段>] --at x,y,z --rot 度 [--scale s | --height 米制高度]`
- 随炮塔转: `--part <炮塔>` + `--like Turret`(按最近模板顶点绑骨); 船体固定: `--like <分段>_DeckHouse` 或 `--hp`。
- 材质: 新建 .mfm 与 `_a/_mg/_n/_ao`, 不与原船共用。单材质 `--texture`/`--color`; 多材质 GLB 自动每材质一个 .mfm(`--materials single` 关掉)。
- 多材质导入: GLB 自带材质且没给 `--texture/--color` 时自动按材质拆(`--materials single` 关掉)。漫反射贴图×系数、`doubleSided`、alphaMode MASK(→`alphaTestEnable`+`alphaReference`=cutoff×255; BLEND 按阈值 128)、自发光(→`_mg` B 遮罩)都带上; 内容相同的贴图只写一份(`_shared*`), 支持 PNG/JPEG/BMP/TGA/DDS。
- `--glow-mat <正则>` 指定材质整块发光, `--b-max 25–50` 大面积微光; `--double-sided` / `--alpha-test N` 对全部材质生效。
- PMX: 骨骼/表情/物理/描边/toon 不转; 需要摆姿势时可选 Blender+mmd_tools(GPL, 自行安装): `scripts/model/pmx_to_glb_blender.py`。PMX 模型多有使用规约, 先确认授权。
- 尺寸: 结果 `units` = 船体长、物件高; 写了 `ship_length_m` 再给米。装饰件通常 < 船长 2%; 先定港口观感米数再反推 `--height`。
- 许可: `pmx2glb`/`import` 会扫描模型目录(及上级)的使用规则(txt/md/html, UTF-8/GBK), 命中「禁止二次配布/再分发」等写 `<glb>.license.json` 并给项目打 `[license] no_redistribute`; `pack` 默认拒绝, 本地 `install` 不受影响。
- 默认写进全部 LOD; 碰撞/装甲不变; 撤销 `model undo`。
- 结果 JSON: `materials[]`、`warnings`、`placement.fit_height`、`units`、`license`; 先看 warnings。
