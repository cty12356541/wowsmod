# workflow.layers.shape.refine.hook: 导回与挂点验证
上级: workflow.layers.shape.refine
下级: 无
---
**输入**：合格 glb、实际部件/挂点与尺度表。
```
wowsmod model import --project <项目> --mesh <道具.glb> --name <唯一名> --section <分段> --like <模板正则> --scale <值>
```
必要时先用 `model remove` 移除目标旧件；核对命令 help 和编辑记录，避免误删。静件、随炮部件和独立动画分别选正确路径；随炮依实际 part/HP 装配，不用独立环绕动画冒充随转。动画实验见 `ref.model.animate`。
**可重复制作**：保留输入 glb、图集、遮罩与参数批次；优先从明确基底建立独立方案，再按记录重放。不要把 Robin apply 脚本或镜像目录命令当所有项目必要步骤，也不覆盖用户现有目录。
**坐标**：核对本次 export/import 的坐标与变换；游戏 y 上、艏 +z，导出工具可能转换轴向，用方向标记与往返小样验证，不能凭另一链路的经验取反。
**检查**：`model list`、实际加载 visual 引用、LOD/材质、日侧多角与炮管活动范围；随后分开记录注册/港口可见性。历史挂载路径与 r32 失踪问题见 `workflow.changelog`。
