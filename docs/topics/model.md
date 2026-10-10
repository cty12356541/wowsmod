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
wowsmod model undo --project <项目>
```
- 坐标是游戏坐标:x 右舷,y 上,z 船艏;单位与 .visual 相同(约 1 = 30 m)。
- 渲染集的编辑默认作用于全部 LOD,所以拉远后不会变回原样。
- 每次编辑都记录在 `edits/` 下,可以 `undo`,也可以用 `replay` 在别的项目上重放。
- 检查:`wowsmod render <项目> --compare --view all`。
