# workflow.layers.shape.blender: 精细改模进 Blender
上级: workflow.layers.shape
下级: 无(命令: `wowsmod help ref.blender`、`ref.model`)
---
**前置**: 先完成 `shape.form-language` 剪影/层级/尺度表。
**何时进 Blender**(CLI `scale/move/rotate/import` 只够粗摆; 下列情况必进, 勿用贴图硬凑外形; 对齐镜学家 Blender/ZBrush):
1. 要**改网格形状**(环截面、翼曲率、徽章厚度、削边/挖洞)。
2. **比例/贴合/穿模**: 相对船长显大显小、悬空或扎进甲板/舰桥、挡炮管回转。
3. 多件对齐/布尔/缝合, 或枢轴不在环心/根部导致 `animate` 公转歪。
4. ZBrush 雕刻后回 Blender 减面再导出。
**不进**: 只平移旋转缩放; 主题只需平面纹样(层 4 UV)。**禁止**为认角色进 Blender 造人物雕像。

**最低细节**(对标例子外带): 倒角/倒边、镶边或分色环、厚度层次; 可浮雕/贴图细节; 发光走独立遮罩或边缘线。禁无无倒角光坨环充数。
**调什么**: 保持 `export-blender` 对象名; 新物体材质名对齐现有 mfm; 坐标 y 上、艏 +z; 主题只改**道具**。装饰件通常 ≪ 船长 2%(`mod set --ship-length`+`units`)。可动炮尽量留原挂点; 做不到写进设计书。

**导出 glb 标准**: glTF Binary; **Apply** 旋转/缩放; 三角化; y 上右手; 单位与 visual 一致(约 1=30 m)或回 import 用 `--height` 定标; 静态网格(不依赖骨骼); 带基本 UV; 非流形/反法线先修。

**往返**:
```
wowsmod model export-blender --project <p> -o ship.glb
# Blender 精修 → 按上表导出 glb
wowsmod model import-blender --project <p> ship_edited.glb
```
或: 道具单独 glb → `model remove` 旧件 → `model import`/`animate`。

**回 import 验收**: `model list` 有道具名、主题无人体材质; `render --warm` 后 day/night 的 side+stern 不穿模、不挡炮、LOD 在、比例合设计书; 发光仍微光。港口无 UI 抽检见 `workflow.layers.verify.port`。

**坑**: 立环/薄环勿对最小轴用 `--height`(会按薄边爆缩放); 改用 `--scale` 或先在 Blender 把包围盒摆正再定标。
**社区对照**: 镜学家精修外带; 竹宛受其指导挂 PnF; 星尘 Maya 整船重导(我们优先补丁式 import, 避丢 LOD)。细节 `ref.blender`。
