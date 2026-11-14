# 实时模拟保真修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Do tasks with scoped ownership and real behavioral verification. You are not alone in the codebase; preserve others' edits and adapt to their interfaces.

**Goal:** 默认模拟视角与真实船体、14 炮位、炮口特效均响应模拟控制。

**Architecture:** Rust 预览装配保留源 visual 树/完整 skin，glTF 标准 skin 传到 Three SkinnedMesh；现有动作和状态流连接真实变形骨及炮口。

**Tech Stack:** Rust formats + preview backend；TypeScript/Three r147；现有 node:test、Rust tests、真实 candidate project 离屏验收。

**Spec:** `docs/superpowers/specs/2026-11-14-preview-simulation-design.md`

## Global Constraints

- 不修改 candidate-project/assets、E:/Korabli 或用户页面；实际新输出放独立 out，父任务负责最终运行部署。
- 不改已审查性能改动语义，不重写 renderer，不迁移 WebGPU，不新增游戏动画导出。
- 不使用整塔 pitch、dominant-weight 近似或 synthetic muzzle 冒充真实动作。
- 子任务按下述文件 ownership；跨界接口先发给负责人，不擅自互改文件。

## Review Focus

- 非零/180° mount 变换下没有两次挂点变换：Task 2、4。
- 同 mesh 两炮位动作互不影响且 geometry 仍共享：Task 2、4。
- 各 renderSet 的单骨 palette=0 不被误映射到 model yaw：Task 1、4。
- 旧 seed/cache、model_key=None 和离线只缓存项目：Task 3。
- 前一帧已 ready 后换目标、切战斗、排队齐射后退出再进入：Task 2、3。

### Task 1: 真正保留模型骨架和混合权重（Worker A / Sol ultra）

**Files owned:** `crates/wowsmod-formats/src/vertex.rs`, `geomedit.rs`, `scene.rs`, `assemble.rs`; 可新增 `crates/wowsmod-formats/src/preview_rig.rs` 并在 lib.rs 注册；`crates/wowsmod/src/m3cmd.rs`。

**Interfaces:**
- `vertex::SkinData { joints: Vec<[u16;4]>, weights: Vec<[f32;4]> }`；`unpack_skin(raw: &[u8], stride: usize, fmt: &VertexFormat) -> Result<Option<SkinData>>`，只解支持的 PC skin。
- `GeoEdit::mesh_skin(&mut self, name: &str) -> Result<Option<SkinData>>`；不必污染用于编辑/导入的 MeshData。
- SceneBuilder 支持带 skin attributes 的 mesh 和 `add_skin(joints: &[usize], inverse_binds: &[M4]) -> usize`、`set_node_skin(node: usize, skin: usize)`（具体复用 ScenePrim.skin 或 add_skinned_mesh 由 A 选最少改动，但交接必须提供最终签名）。
- `AssembleOpts.preview_rig` 默认 false，`assemble --preview-rig` 开关；GLB 属性/节点 extras 按设计固定。

- [ ] 先写支持 raw `[0,3,6,0,0,63,192,0]`、单骨 palette、非法 stride/index/weight 的测试；确认旧代码无法满足。
- [ ] 实现源字节解包、renderSet→model palette remap、完整 source hierarchy 克隆与 per-mount skins；静态 hull/动画对象沿旧路径。
- [ ] 合成 visual+geometry fixture 至少两个相同炮型 mount，各有独立骨架，glTF asserts 骨/炮口层级、joints/weights/IBM 与 mesh sharing；支持混合顶点和单骨 renderSet。
- [ ] 执行 `cargo test -p wowsmod-formats` 与 CLI 相关测试；独立 out 装配实际 project，记录14炮位/权重非法计数=0。
- [ ] 报告实际 API/schema 和生成 GLB 路径给 Worker B/C；不提交其他 worker 文件。

### Task 2: Three 蒙皮、真实动作、默认视角（Worker B / Sol ultra）

**Files owned:** `preview_app/web/src/lib/glb.ts`, `engine/viewer.ts`, `engine/sim.ts`, `engine/shaders.ts`; 新 `lib/glb-skin.test.mjs`, `lib/simulation-motion.test.mjs`, `lib/simulation-camera.test.mjs`。

**Interfaces:** 消费 Task1 标准 GLB skin + existing extras.bone/muzzle/rig。保留 Viewer 现有 public setWaves/setCamMode/setCombat/fire/simReset 方法；`getView()` 不改变形状。必要新增 `getSimulationCapabilities()` 与精确声明后通知 C。

- [ ] 先加真实 Three SkinnedMesh fixture 测试：有平移及180°旋转父 mount、两独立 skeleton、同 geometry；测试 boneTransform 得到正确世界顶点，权重混合算式而非 UI 数值。
- [ ] 两阶段 parseGLB，加载 JOINTS/WEIGHTS/skin/IBM，显式 identity bind、attached；适当 disable skinned frustum culling，去重 dispose Skeleton。
- [ ] setupMounts/poseMount/stepMount/readyMounts/fire 使用真实 rig，fire 前目标状态刷新但不瞬转；pitch就绪纳入判定；退出/重置清 pendingShots，炮口矩阵在 emit 前更新。
- [ ] 默认 sim preset 首次应用，user camera 后续保留；loadGeometry preserveCamera、切标签、reset 都不抢镜。水面世界参照沿现有 P/psi 表现航行；free 模式继续空间固定。
- [ ] 验证 yaw-only 点不随 pitch，barrel 点随 pitch/recoil，全部混合点等于源公式，muzzle 与粒子初始位置/方向一致；过射界不得 fire；首 tick 不虚报；重载/退出清理正确。
- [ ] 执行新增 node tests、现有 npm test、npm run typecheck；交接实际 Three 变形证据与控制能力。

### Task 3: 后端缓存与控制状态一致（Worker C / Sol ultra）

**Files owned:** `crates/wowsmod/src/preview/state.rs` 及相邻目标测试；`preview_app/web/src/store.ts`, `lib/types.ts`, `components/SimPanel.tsx`, `lib/i18n.ts`，状态同步测试。

**Interfaces:** fallback 改用 A 的 --preview-rig；GLB asset.extras.previewRigVersion=1 与 mount extras.rig。现有 Viewer setters 不变；若 B 新增 capability 方法需先按实际定义接入。

- [ ] 写缓存旧版+model_key=None、有编辑缓存、离线静态缓存三类回归；修改 generator key/version 控制所有复用分支。postprocess 保留 skin 并只标记真正顶层 part。
- [ ] 避免 sim/parts 数据“声明骨存在就可用”；由实际几何能力交集决定正常动作支持，缺失 AA/rudder/LOD 明确展示不可用。
- [ ] store 抽统一 applySimulationSettings，在 load/enter/reset 调用完整 settings，含 waves；reset tel/rudder=0，waves等保持。退出 game 模式同步 camMode，清空模拟时同步必要store状态。
- [ ] 状态行为测试：waves=0.35 后 reset/reload/切工作区仍一致；game退出UI=orbit；无rig不正常fire；将已ready目标改到需俯仰后立即fire返回0。
- [ ] 执行 Rust preview tests、npm test、npm run build。只整理本任务文件交接。

### Task 4: 集成与关键审查（Root + fresh Astra ultra reviewer）

- [ ] 等 A/B/C 都通过单元验证，按真实 project 用新 binary 生成独立 geometry；保留历史缓存备份，不写游戏目录。
- [ ] 构造真实 Three 离屏 Viewer，逐一验证3主+11副：每炮位选择 source yaw-only、pitch或recoil、mixed三种存在的顶点，比较 source evaluator 与 boneTransform；并检查真实 muzzle 个数/位置与发射方向。
- [ ] 用 full+满舵、waves0/非0、combat瞄准、single/salvo、reloadReal 开关、reset/离开/重入脚本走 public actions；记录矩阵/顶点/粒子/wake变化。只更改数字或Group rotation不算通过。
- [ ] 验收相机首入视角与用户参考相符，旋转后跨参数/标签/缓存刷新保留；用 computer-use 专员检查用户可见结果（不要操作未授权页面）。
- [ ] 重跑与变更匹配的 cargo/npm tests 和 release build；确认性能优化仍存在且实际模型装载耗时、draw calls/帧率无显著异常。
- [ ] fresh Astra review 聚焦格式真实性、绑定矩阵、缓存所有分支、状态契约与未支持动作真实性；修复发现后只重跑相关项。

设计与计划已自查覆盖目标、类型交接、缓存和真实顶点验收。当前 source skin 格式已有充分证据，不存在阻止14炮位真实蒙皮实现的数据缺失；唯一需父任务补齐的是精确当前相机 pos/tgt（若确需一比一视角），独立舵叶/AA/LOD不是当前资产可承诺功能。

## 实际派发调整

Rust formats、CLI、preview cache 全部由 Astra /root/sim_rig_design 负责；Task 2 和 Task 3 的前端代码全部由 Sol /root/sim_frontend 负责。root 负责生成产物与发布；独立的 Sol /root/library_style 仅负责 library CSS/展示。无需重复确认既定需求。机位按用户截图方向近似复刻，缺少精确 pos/tgt；不以像素一致作为承诺。已有航行参照足够时不新增水面刻度。

