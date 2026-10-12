# workflow.layers.shape.refine.hook: 导回挂点与动画
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: 合格 glb; design 挂点表。
**做法**:
```
wowsmod model remove --project <p> --renderset <旧名>   # 必要时
wowsmod model import --project <p> wing.glb --name wing_p2 --scale …
wowsmod model animate --project <p> --name turret_shell --orbit …  # 仅随炮件
```
**规则**: renderset 名唯一(删后同名仍占) → **确定性重建**: 每轮 robocopy /MIR 贴图定稿目录→新目录, 按 `props_spec.json` 逐件 import(`scripts/model/robin/win/apply_p3.ps1`)。静件 `--section MidFront --like '^MidFront_DeckHouse$'`; 随炮件 `--part <完整部件名> --like Turret`(该型号全部实例都有)。
**坐标**: 游戏 y 上、艏 +z; `model export` 的 glb 是艏 −z(取反)。Blender 里 X=x, Y=艏 z, Z=上, glTF 导出往返正确。
**完成标准**: `model list` 见齐名; 无人体材质; 日侧图不挡炮管回转包络。
