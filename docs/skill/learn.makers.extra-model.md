# learn.makers.extra-model: 外带建模
上级: learn.makers
下级: 无
---
**社区做法**: ContentSDK 导出明文 `.geometry/.visual/.model` → gmConverter3D / Blender 插件转 obj 或 glTF → 加件、改名 `*Shape`、对齐 `.visual` 渲染集 → GeometryPack / MK Model Export 回写 → 新件配 `.mfm` + `_a/_mg`。
**挂法**: 新渲染集写进分段 `.visual`(单位矩阵、顶点已在分段坐标); 或独立 `misc/*.model` 再由动画/节点引用。保留 crack/patch 与 LOD(社区整船重导常丢 LOD——我们默认补全)。
**主题模式(产品硬规则)**: 只允许**道具形**外带件(饰板、翼片、环、徽章板、麦克风、耳羽等)。**禁止**人物全身/半身 mesh 当雕像。PMX 可本机拆 UV/取色/只导出道具。见 `workflow.theme` / `workflow.layers.shape`。
**复刻模式**: 参考视频/包若含人物模型(如流萤定制), 设计书写「参考含人物」并经用户确认后才可做; 默认仍禁。
**对本工具**: `model import` / `pmx2glb`(去人体材质) / `model animate` / `workflow.layers.shape.blender` / `ref.blender`。尺寸用 `mod set --ship-length` + `units`(装饰件通常 ≪ 船长 2%)。完成标准见层 3。
**来源方向**: 莱服「创建模型修改」帖; thatsinkingfeeling `.geometry-converter`; wows-extractor-gui; SEA-group 成品 PnF 安装结构。
**作者实践**: 镜学家-深见用 **Blender/ZBrush** 精修外带(可动炮/透明壳/多翼), 并指导竹宛挂 PnF; 精细网格走 `workflow.layers.shape.blender`。星尘 Maya 整船重导仅作复刻参考(易丢 LOD)。见 C 线。
