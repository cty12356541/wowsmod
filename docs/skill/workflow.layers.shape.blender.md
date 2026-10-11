# workflow.layers.shape.blender: 精细改模进 Blender
上级: workflow.layers.shape
下级: 无(命令: `wowsmod help ref.blender`、`ref.model`)
---
**何时进 Blender**(CLI `scale/move/rotate/import` 不够时必进, 勿用贴图硬凑外形):
1. 外带件要**改网格形状**(翼片曲率、环截面、徽章立体厚度、削边/挖洞)。
2. 多件**对齐/布尔/缝合**, 或相对船体精确贴合甲板/舰桥曲面。
3. 需要 **ZBrush/雕刻**级细节后再回 Blender 减面(镜学家路线)。
4. 复刻参考含复杂外壳/可动炮罩, 且用户确认要做几何。
**不进**: 只平移旋转缩放已有件; 主题只需平面纹样(走层 4 PS/UV)。
**调什么**: 保持 `export-blender` 给出的对象名; 新物体材质名对齐现有 mfm 模板; 坐标 y 上、艏 +z; 主题只导出**道具**部件。可动炮尽量绑原骨骼/挂点; 做不到在设计书写妥协。
**往返**:
```
wowsmod model export-blender --project <项目> -o ship.glb
# Blender 编辑 → 导出 glb(同名对象写回)
wowsmod model import-blender --project <项目> ship_edited.glb
```
**验收**: `model list` 看新渲染集; `render --warm` 后工作台日/夜同角; 不穿模、不挡炮管、LOD 仍在; 主题无人体材质装配。港口最终见 `workflow.layers.verify.port`。
**社区对照**: 镜学家 Blender+ZBrush 精修外带; 竹宛受其指导挂 PnF; 星尘 Maya 整船重导(我们优先补丁式 import, 避丢 LOD)。命令细节 `ref.blender`。
