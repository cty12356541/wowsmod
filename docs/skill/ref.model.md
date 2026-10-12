# ref.model: 几何编辑命令
上级: ref
下级:
- ref.model.import: 导入外部模型(人物/道具, 多材质 GLB / PMX)
- ref.model.animate: 让独立物件自转/环绕/浮动
---
```
wowsmod model list --project <项目>                       # 分段/渲染集/部件/挂点
wowsmod model scale 1.5 --project <项目> --part FGM049     # 部件全部实例
wowsmod model scale 1.3 --project <项目> --hp HP_FGM_3     # 单个挂点
wowsmod model move --by 0,0.05,0 --project <项目> --renderset '^MidBack_Net$'
wowsmod model rotate --deg 0,90,0 --project <项目> --hp HP_FGS_1
wowsmod model remove --project <项目> --renderset <正则>   # 或 --part / --hp
wowsmod model prune --project <项目> --renderset '^(Bow|MidFront|MidBack|Stern)$' --facing side --max-size 0.08 --dry-run   # 原壳降噪: 删游离小块(全 LOD, 只退化三角形), 见 shape.refine.clean
wowsmod model replace --project <项目> --part A --with B   # 或 --glb 网格
wowsmod model add --project <项目> --glb new.glb --like <渲染集>   # 共用模板材质
wowsmod model import --project <项目> --mesh wing.obj --like Turret --part FGM049 --at 0.4,0.12,-0.2 --glow mask.png   # 新部件+独立材质/贴图, 见 help ref.model.import
wowsmod model import --project <项目> --mesh figure.glb --like Turret --part FGM049 --height 0.06   # 多材质 GLB: 每个 glTF 材质一个 .mfm
wowsmod model pmx2glb figure.pmx -o figure.glb --pose arms-down:55   # MMD → GLB; --pose 静态摆姿势(骨^度 / 骨=rx,ry,rz)
wowsmod model undo --project <项目>
```
- 坐标是游戏坐标:x 右舷,y 上,z 船艏;单位与 .visual 相同。米/单位: `wowsmod mod set <项目> --ship-length <真实舰长 m>` 后, `model list` 的 `units` 直接给 `hull_length_units`、`m_per_unit` 和 1.0/1.4/2.0 单位的米数; `model import/animate` 结果也带物件米高。
- 渲染集的编辑默认作用于全部 LOD,所以拉远后不会变回原样。
- 每次编辑都记录在 `edits/` 下,可以 `undo`,也可以用 `replay` 在别的项目上重放。
- 检查:`wowsmod render <项目> --compare --view all`。
