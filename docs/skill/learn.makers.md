# learn.makers: 社区制作方法总览
上级: learn
下级:
- learn.makers.texture: 官方 UV 重画、`_a/_mg/_n`、迷彩遮罩
- learn.makers.extra-model: 外带几何 / Blender·geometry / 主题禁立人像
- learn.makers.emissive-pnf: `_mg` B 发光、emissive 材质、PnF 注册
- learn.makers.sdk-pipeline: ModsSDK extractSources → registerShipMod
- learn.makers.pitfalls: 版本路径、mip、迷彩冲突、莱服解包坑
---
**目标**: 把社区教程压成可执行类别, 链到本工具命令; 不重复堆 `workflow`/`ref` 细节。
**四类做法**:
1. **贴图痛船** → `learn.makers.texture` → 层 4/5、`ref.recolor`
2. **外带建模** → `learn.makers.extra-model` → 层 3、`workflow.layers.shape.blender`
3. **发光 / PnF** → `learn.makers.emissive-pnf` → 层 5、`ref.glow`
4. **SDK 管线** → `learn.makers.sdk-pipeline` → `ref.project`
**主题产品规则(与社区观察对齐)**:
- 高质量主题船通常 = **官方 UV 重画**(角色色/纹样/徽章画进 `_a`) + **道具形外带件**(翼片饰板、环、麦克风等)。
- **主题设计模式禁止默认立人像**(整身/半身挂甲板); 流萤等定制包含人物是**复刻例外**, 须在设计书写明并获用户确认。
- 人物插画可贴在 UV 平面上作为图案元素(仍是贴图, 不是 3D 雕像)。
**来源摘要**: `e2e/learn/MAKERS_NOTES.md`(A/B/C/D)。官方 ContentSDK+莱服帖+中文路径; **C**: 夙岚 UV/发光、竹宛挂模、**镜学家 Blender/ZBrush**、星尘拆包、**茄宝 PS 画 UV**教学。精细改模 → `workflow.layers.shape.blender`。
**完成标准**: 能指出当前任务属于哪一类、该读哪个子节点与哪一层。
