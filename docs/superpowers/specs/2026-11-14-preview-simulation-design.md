# 实时模拟保真修复设计（2026-11-14）

## 目标与边界

把用户当前“船艏左下、船尾右上、斜俯看甲板”的视角作为初始模拟视角；用户随后旋转、缩放、平移应保留。右侧控制必须驱动真实预览几何：船体航行/横摇、14 座主副炮的旋转/俯仰/后坐、随真实炮口位置和方向生成的开火效果。现有 WebGL/Three r147 可完成；不迁移 WebGPU，不修改项目资产，不生成或安装游戏动画。

根因已确认：fallback assemble 把每炮型烘焙为静态共享 mesh，丢掉 .visual 层级和顶点 skin。sim.json 声称支持骨骼，Viewer 实际只改变数值。ScenePrim 与自定义 parseGLB 目前均不携带 skin。

## 方案选择

1. **采用标准 glTF skin + Three SkinnedMesh**：真实源骨、真实混合权重，模型 geometry 可共享、每炮位骨架独立。小幅扩展现有装配器与解析器，不动渲染架构。
2. 精确刚性子组：可处理 FGS036/FGS053，但主炮 FGM049 含真实混合权重而不可无损处理；没有必要并行维护另一条路径。
3. 整炮塔套 yaw/pitch 或 dominant-weight 分组：会让炮座随炮管俯仰或改变混合区，不符合本轮真实性要求。

## 格式证据与确定的字节契约

独立探针 `skin_probe.rs` 用当前 formats rlib 解码只读资产，结果见 `skin-probe.txt`。当前主炮 Turret 4317 顶点中 309 个混合顶点；Turret_templates 288 顶点中 278 个混合顶点。三个实际炮型的节点索引、权重和均合法。实际源文件位于 `E:/WoWSModding/out/compat-26.11-20261113/candidate-project/assets/res_mods/PnFMods/Robin_Bourgogne/FSB510_Bourgogne/gun/`。

BigWorld 原始结构明确 PC 8 字节顺序；原始软件蒙皮实现明确索引除以 3 与隐含第三权重：

- [vertex_formats.hpp](https://raw.githubusercontent.com/v2v3v4/BigWorld-Engine-14.4.1/master/programming/bigworld/lib/moo/vertex_formats.hpp)，VertexXYZNUVIIIWWPC 806–818、VertexXYZNUVIIIWWTBPC 932–944。
- [software_skinner.hpp](https://raw.githubusercontent.com/v2v3v4/BigWorld-Engine-14.4.1/master/programming/bigworld/lib/moo/software_skinner.hpp)，SoftSkinVertex 324–344。

对支持的 set3/*iiiww*pc，skin 起点 b：

```
JOINTS_local = [b[2]/3, b[1]/3, b[0]/3, 0]
WEIGHTS = [b[6], b[5], 255-b[6]-b[5], 0] / 255
```

要求已知格式、stride 匹配、完整 8 字节、索引整除 3、有效 palette 索引、非负隐含权重。零权重可统一到合法第 0 骨。未知格式/缺失骨应降级为明确静态能力并给出原因，禁止造假蒙皮。不要沿用本地 wows-toolkit 的 first-byte/dominant 近似。

## 装配与 GLB 契约

- 新增 `AssembleOpts.preview_rig: bool` 与 CLI `assemble --preview-rig`，默认 false。preview fallback 调用 `--full --preview-rig`，避免改变现有模型编辑/Blender 往返默认流程。
- 顶层炮位仍保留现有 id/name 与挂点矩阵；每炮位独立复制完整 visual node tree（或所有变形骨/炮口及其祖先的完整闭包），保留 source local matrix。
- source `Rotate_Y/Rotate_X/Roll_Back1` 分别标记 `extras.bone = yaw/pitch/recoil`；严格 `HP_gunFire\d+` 为 `extras.muzzle` 零起编号，排除 `HP_gunFireEffect`。保留 source 名可诊断。
- 模型所有 renderSet 的节点 palette 合并为稳定 model palette；每个 renderSet 读取自己的 palette 后 remap JOINTS 到 model palette，不能把单骨 Turret_Gun9 的本地 0 错当 yaw。
- `JOINTS_0` 用 UNSIGNED_SHORT VEC4，`WEIGHTS_0` 用 FLOAT VEC4；同炮型 mesh/accessor 仍共享，每个炮位有独立 glTF skin.joints 指向自己的节点索引。
- **保留 BlendBone 补偿节点时 inverseBindMatrices 为单位矩阵**：源 skin 直接乘这些节点的世界矩阵，它们自身已经包含逆绑定补偿（源静止矩阵与单位阵误差约 1e-4）。不能再次用 Rotate_X/Roll_Back 世界逆矩阵补偿。
- glTF 仍应用现有 `C M^T C` 与 z 翻转/绕序规则。输出顶层 `asset.extras.previewRigVersion = 1` 和挂点 `extras.rig = { version: 1, status: 'skinned', bones: [...], muzzles: N }`。没有真实绑定时 status 为 static，给出 reason；不得声称动作可用。
- postprocess 材质不改变 skin/accessor/bin；只给真正顶层挂点添加 part 元数据。不能把嵌套 HP_gunFire* 识别为独立 part，也不能覆写 bones=[]。

## Three.js 契约

- parseGLB 两阶段建立 node index → Object3D 映射，再连层级，再创建 skin。被 joints 引用的节点是 Bone；其他源节点可 Group，保留 extras。
- 存在 node.skin 的 primitive 创建 SkinnedMesh，设置 skinIndex/skinWeight；读取 MAT4 IBM，缺省 IBM=单位阵。传入显式 identity bindMatrix，不允许 bind() 隐式 calculateInverses 抹掉源补偿语义。保持 attached 模式，连接完整层级后 updateMatrixWorld(true)。
- mesh 同一型可共享 BufferGeometry；不同 mount 的 Skeleton/bones 绝不能共享。清理时 Geometry/Skeleton 去重 dispose；骨架贴图也回收。
- 现有 MeshStandardMaterial/onBeforeCompile 保留标准 skinning shader chunks，可正常蒙皮；A/B/选中/高亮同样使用真实 SkinnedMesh。
- r147 的 CPU 顶点验证使用 `mesh.boneTransform(index, position.clone())` 后乘 mesh.matrixWorld。不要用 geometry.position 是否改变来判断 GPU 蒙皮；也不要用 UI 数值代替顶点。
- 动态炮塔的包围盒/裁剪需保守（小规模炮塔 skinned mesh 设置 frustumCulled=false）；真实 raycast/选中按 r147 能力验证，必要给这些 mesh 实现基于 boneTransform 的射线测试，不能把静止包围盒当动态精确结果。

## 模拟、状态与默认视角

- setupMounts 根据加载后的实际节点与有效蒙皮检测能力。14 座现有主副炮必须全部有真实 yaw/pitch/recoil/muzzles；unsupported 模型只允许明确标为“特效位置估计”的兼容展示，不计入正常就绪/开火。
- 就绪统一为 simOn、combat、有有效目标、真实炮口与 rig、inSector、aimed（yaw 和 pitch 均 <1°）、reload<=0。调用 fire 前同步计算当前目标的限位与误差，消除切战斗首 tick 假 ready；不能瞬间把几何转到目标。
- 先 step/pose/updateMatrixWorld，再读取 muzzle 生成粒子。单发/齐射现有 40ms 排队保留；退出模拟/重置取消排队，避免回到模拟补发旧炮弹。重置时姿态立即回归；涌浪、真实装填、相机等设置和 store 保持一致。
- store 一个 applySimulationSettings 路径供进入模拟、load sim、reset 调用；包含 waves。reset 只把动态航行状态与 tel/rudder 归零，保留 waves 等用户设置。退出 game camera 时同步 store.camMode='orbit'；再次进入不要显示与实际不符的模式。
- 使用独立 `sim` camera preset，优先由已记录的当前 pos/tgt 提取归一化方向再 fitCentered，满足响应布局。首次进入模拟且无用户手动操作时应用；后续切标签、改参数、reset、材质/geometry 刷新都不抢相机。用户手动 setView/orbit/controls start 后保留其选择。若没有当前数值，父任务需根据用户现有截图校准再验收，不能宣称精确复刻。
- 保留已实现的虚拟世界坐标（船位固定原点、海水/尾迹反向运动），避免全面变换体系重写。跟随视角注明含义；利用现有 P/psi 在模拟水面显示低干扰世界固定参照/尺度线，使前进和转向可见，并保留 free 模式观察船相对固定镜头移动。不能将 shipFrame.y 旋转与现有逆世界变换同时启用造成重复转向。
- 不宣称当前未提供的独立舵叶、AA 或 LOD 几何运动；UI 按真实能力禁用/说明。

## 缓存、兼容与验收

preview generator version 必须参与所有 cache 分支（包括 model_key=None 和已有 seed/cache），不能仅修改模型文件指纹。使用独立 generator version sidecar 或合并 key 均可。自动升级仅适用于有 model_key/本生成器 provenance 的派生缓存，或已确认的内置 seed 副本；来源不明的无 key 缓存可能是用户导入几何，必须保留而不覆盖。派生旧缓存先成功生成/校验 GLB 后原子替换，再写标记。无法重建的离线缓存继续静态可看并明确能力不足，不能直接标新版本假装具备 rig。模型 hash 变化仍沿用已审查的前端 generation/identity 保留逻辑。

完成标准：实际 candidate project 3 主 +11 副全部真实绑骨；源 evaluator 和 Three 顶点变换误差 <1e-5 模型单位（f32 累积容差必要时按数值尺度放宽并说明）；非零 mount 平移/旋转无双变换；yaw-only 顶点不随 pitch/recoil，barrel 顶点随 pitch/recoil；混合顶点等于各骨线性混合；每个真实 muzzle 跟随并是火焰初始位置/方向依据。FGS036 的自定义 rb_shell 在源文件就是 pitch 绑定，应保留，测试应按 source weights 选顶点，不能凭“外壳”名称重写作者绑定。
