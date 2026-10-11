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
**规则**: renderset 名唯一(删后同名可能仍占 → 换新名); 随炮件绑炮塔节点; 静件绑舰桥/桅/甲板。坐标 y 上、艏 +z。
**完成标准**: `model list` 见齐名; 无人体材质; 日侧图不挡炮管回转包络。
