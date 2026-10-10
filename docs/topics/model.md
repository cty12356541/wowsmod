# model: 几何编辑(SDK 项目)

```
wowsmod model list --project <项目>                       # 分段/渲染集/部件/挂点
wowsmod model scale 1.5 --project <项目> --part FGM049     # 部件全部实例
wowsmod model scale 1.3 --project <项目> --hp HP_FGM_3     # 单个挂点
wowsmod model move --by 0,0.05,0 --project <项目> --renderset '^MidBack_Net$'
wowsmod model rotate --deg 0,90,0 --project <项目> --hp HP_FGS_1
wowsmod model remove --project <项目> --renderset <正则>   # 或 --part / --hp
wowsmod model replace --project <项目> --part A --with B   # 或 --glb 网格
wowsmod model add --project <项目> --glb new.glb --like <渲染集>   # 共用模板材质
wowsmod model import --project <项目> --mesh wing.obj --like Turret --part FGM049 --at 0.4,0.12,-0.2 --glow mask.png   # 新部件+独立材质/贴图, 见 help design-shape
wowsmod model import --project <项目> --mesh figure.glb --like Turret --part FGM049 --height 0.06   # 多材质 GLB: 每个 glTF 材质一个 .mfm
wowsmod model pmx2glb figure.pmx -o figure.glb            # MMD 模型 → GLB(纯 Rust, 静态网格)
wowsmod model undo --project <项目>
```
- 坐标是游戏坐标:x 右舷,y 上,z 船艏;单位与 .visual 相同(约 1 = 30 m)。
- 渲染集的编辑默认作用于全部 LOD,所以拉远后不会变回原样。
- 每次编辑都记录在 `edits/` 下,可以 `undo`,也可以用 `replay` 在别的项目上重放。
- 多材质导入: GLB 自带材质且没给 `--texture/--color` 时自动按材质拆(`--materials single` 关掉)。漫反射贴图×系数、`doubleSided`、alphaMode MASK(→`alphaTestEnable`+`alphaReference`=cutoff×255; BLEND 按阈值 128)、自发光(→`_mg` B 遮罩)都带上; 内容相同的贴图只写一份(`_shared*`), 支持 PNG/JPEG/BMP/TGA/DDS。
- `--glow-mat <正则>` 指定材质整块发光, `--b-max 25–50` 大面积微光; `--double-sided` / `--alpha-test N` 对全部材质生效。
- PMX: 骨骼/表情/物理/描边/toon 不转; 需要摆姿势时可选 Blender+mmd_tools(GPL, 自行安装): `scripts/model/pmx_to_glb_blender.py`。PMX 模型多有使用规约, 先确认授权。
- 检查:`wowsmod render <项目> --compare --view all`。

## 动画物件(model animate)
`wowsmod model animate <项目> --section Stern --like DeckHouse --mesh x.glb --preset spin|orbit|float --at x,y,z [--rpm 6 --axis vertical|x|z --radius --orbit-rpm --phase --float-amp --period --height --glow-mat 名 --b-max --all-lods]`
产出 `misc/<名>.{geometry,visual,model,seq,ssm}` + 材质, 并把 .ssm 挂到分段 .model 的 `<animations>`。轨道按每 1/4 圈打关键帧、切线=斜率, 线性/Hermite 插值都匀速。核对: `model anim-sample <seq>` 逐秒打印位置/角度。工作台装配时把动画物件放进场景并循环播放(帧采样 ≤240, 超过 60 s 的序列只播前 60 s)。
未实证(待港口): 欧拉角单位/顺序、分段 .model 是否接受 SDK 格式的 `<animations>`; 不行再改原生格式。

