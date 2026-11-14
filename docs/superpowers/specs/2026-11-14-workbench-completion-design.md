# 制作工作台完成设计

日期：2026-11-14。核对基线：`f094f31`。本文是实施约束，不是功能已完成的证明。

## 用户授权与范围

用户要求推进到无残留任务，并明确“除了 mod 本身的升级，工作台的全部都要”。因此完成 `docs/requirements-status.md` 后续候选 1、3–7，补回原制作库设计遗漏的真实最近修改时间，以及模拟能力说明和 WebGPU 技术评估。Mod 美术、原始资产升级、游戏内动画导出和真实客户端重新安装不属于本轮。

沿用现有 Rust 本地服务、React 工作台、Three.js r147 渲染器和已登记的稳定项目 ID。已有明确授权，依据用户 AGENTS.md 直接实施，不重新请求设计许可。以下工程取舍由实现者作出，不表述为用户逐项选择过。

## 现状与选择

现有库列表直接读取轻量项目元数据，登记身份与路径分离；`PreviewService` 按项目创建会话。封面服务已有串行队列、按项目和输入失效的 640×360 `deck` 图。扫描现在立即登记。历史面板显示 edits 步骤，`undo/restore` 明确为 false，不能作为完整文件快照。模拟使用真实蒙皮，但 UI 尚未区分全部无法开火的原因，LOD 控件固定列出 0–3。性能和 WebGPU 的结论需要各自证据。

采用独立功能模块加少量入口集成。另一种全局重构方案会同时改变项目、渲染和历史语义，增加回归范围；继续在旧 History 的禁用按钮上补标签也不能形成可恢复快照。检查点选用“保存完整可管理源文件 → 差异 → 恢复为新方案”，以无需覆写当前项目的方式交付完整取回能力。原地撤回既有编辑步骤不冒充检查点恢复。

## 共同约束

- 原 Mod 和真实客户端只读；需要资产变化的验证只能使用隔离副本。
- 浏览、扫描预览、对照、记录实机结论和保存检查点均不得隐式安装到游戏。
- 不因打开项目、收藏、标签、库更新时间或预览缓存变化伪造源修改时间。
- 所有新状态按稳定 `project_id` 隔离；重定位保留历史，复制并登记取得新 ID。
- 原 `preview <path>`、`render`、MCP、CLI `library scan` 和旧 HTTP `/library/scan` 保持兼容。
- 列表不得等待 GLB 装配、图片解码、浏览器启动或全项目内容散列。
- 不跟随源目录中的符号链接、Windows junction/reparse point；不静默跳过会影响完整性的源读取错误。
- 保留模型、材质、蒙皮和高画质能力；性能结论必须使用相同机位、实际像素尺寸、DPR 和画质条件。
- 新文案覆盖中文、英文、俄文；复用现有浅灰/白/蓝视觉、键盘操作和窄屏布局。

## 1. 真实最近修改时间

`LibraryEntryView` 增加 `modified_unix: Option<u64>`，只在 list 时派生，不写进 `library.json`。取 `mod.toml`、可选 `fx.json` 和 `assets/res_mods` 内实际普通源文件及目录的最大 mtime，Unix 秒；资产目录时间用于观察增加/删除文件这一真实变动，项目根目录时间不计。它描述源文件系统时间，不是内容 revision，也不能保证跨机器时钟一致。

遍历需要有界、不跟链接、检查每级路径。源缺失、无权读取、时间无效或超出边界时返回 null 并给出警告，不能把不完整遍历的最大值展示为准确值。没有资产但有合法 `mod.toml` 时可以使用后者的时间。默认最近打开排序保持不变，新增 `sort: 'modified'`；降序、null 最后、相同时按项目 ID 稳定排序。卡片同时保留“最近打开”和“源文件修改”。

## 2. 内容修订与兼容性

### 内容修订的共享合同

新增 `project_revision.rs`。内容修订与旧 steps ID、mtime 指纹分开；revision 是 `sha256:<64 hex>`。规范化、按相对路径排序的文件清单记录 `{path, bytes, sha256}`，revision 散列 schema 与清单，不散列绝对根目录或时间，故单纯搬家不改变 revision。

管理范围是项目根目录内普通源文件，必须含 `mod.toml`，包括 `assets/res_mods/`、`edits/`、`fx.json`、`design/` 和作者其他普通源文件。根层排除 `.git`、`.wowsmod`、`.install`、`.render`、`renders`、`out`、`target`、`node_modules`；这份排除规则带 `scope_version: 1` 并在检查点 UI 展示。外部 SDK/游戏素材不追踪、不复制，外部依赖作为警告和构建信息保存。链接、不可读文件、路径冲突或达到限额使创建失败，不能发布不完整快照。初始界限为深度 64、100,000 个普通文件、总计 16 GiB，达到上限报可操作错误，不截断。

类型与函数：

```rust
pub struct SourceFile { pub path: String, pub bytes: u64, pub sha256: String }
pub struct SourceManifest { pub scope_version: u32, pub revision: String, pub files: Vec<SourceFile>, pub total_bytes: u64 }
pub fn read_source_manifest(root: &Path) -> Result<SourceManifest>;
```

散列时检查文件和目录身份，结束后核对源仍稳定；变化返回 `source_changed`，由用户刷新/重试。这个 API 只用于用户打开详情、记录或保存检查点，不能直接放入每次库列表。

### 实机记录

`workbench_records.rs` 在 `WOWSMOD_HOME/workbench-records/<安全的项目键>/verifications.json` 原子保存带 schema 的记录；损坏保留原件并报错。每条记录有 `id`、`project_id`、`revision`、`build`、`outcome: passed|issues|failed`、`scene`、`note`、`evidence: string[]`、`recorded_unix`。证据只保存用户输入的路径或 URL 文本，不自动打开、下载或假装核验。

```text
GET  /api/v1/library/projects/:id/compatibility
→ {ok,project_id,project_build,target_build,current_revision,install,
   records,matching_record_id,warnings}
POST /api/v1/library/projects/:id/verifications
{expected_revision,build,outcome,scene,note,evidence}
→ {ok,record}
```

POST 服务端重新计算 revision，和 `expected_revision` 不同返回 409 `source_changed`，不把记录附到新内容；build、scene 非空且有长度界限。记录人可以报告别的构建，UI 默认当前目标构建并明确可编辑。记录仅表示用户的观察，不表示工具自动运行了游戏。以项目 ID、内容 revision、build 三者同时匹配选取最近记录，最新失败覆盖先前通过；缺记录显示“未记录”，不同 build/revision 显示“其他构建/历史修订”，不能继续显示当前已通过。

库卡片的摘要展示制作构建、当前目标构建、真实安装状态、最近一条历史实机记录。尚未计算当前内容 hash 时，只陈述该历史记录的 build、revision 和日期，不给“当前已验证”徽章。详情请求计算 revision 后才给匹配关系。`working/review/complete` 仍是用户制作状态，不推导兼容性；已应用也不等于实机通过。

旧 apply dry-run 补验证：假客户端目标 build=123、项目制作 build=122，断言 `target_build` 与升级警告，项目和客户端字节均不变。真实客户端不重新安装。

## 3. 两套同船方案的固定视角对照

新增 `CompareProjects`，制作库单船方案页和工作台导航都有明确入口。两个独立选择器按 ID 选择同一非空船 key 下不同的项目；同名项目仍显示独立 ID/路径提示。无第二方案明确说明；缺失项目/图保留占位和原因，不能拿另一边的图代替。

现有封面只作为即时占位，并明确标记“封面预览，共同机位图生成中”；它们各自按 bbox fit，不能作为最终严谨对照。最终两张图必须共享数值相机、640×360 实际像素、DPR 1、high、日间、studio、LOD 0、时间 t=0，图在两栏相同显示比例且完整呈现，窄屏堆叠。两张一致图全部成功后原子切换展示，不能左边新对照右边旧封面却仍称同机位。

### 渲染和共同相机

源码已有可用基础：`render.ts` 的 `setup` 支持 custom 的 az/el/dist/tgt/fov，`Viewer.setView(pos,tgt)` 可以直接设数值相机，`cdp::Page::render` 返回 PNG 和完整 meta。当前 `RenderResult.meta` 尚无相机，`Viewer.getView()` 会四舍五入到3位，不用于精确合同。新增 `RenderParams.pos?: string`，parseQuery 接受逗号分隔三数，和已有 tgt/fov 组合时以 `setView` 设置，不再独立 fit。显式相机不能与 focus/自动 dist 同时执行；无效、NaN/Infinity、零观察方向返回 bad_param。

`RenderResult.meta` 增加未舍入 `camera:{pos:[number,number,number],tgt:[number,number,number],up:[number,number,number],fov,near,far,aspect}` 与 `bounds:{min:[number,number,number],max:[number,number,number]}`。近远裁面也允许精确传入，确保两边投影矩阵相同。先在两边 scoped 页面读取 deck 和 bounds，取 A 的 deck 观察方向、两侧 bounds 并集的中心为 target，用并集8角点计算一次共同 distance，保持 fov=34、aspect=640/360、fill=0.87；扩大共同 distance 直到所有角点落入 fill 区且在近远平面内。两个最终请求都显式传同一 camera，绝不再单边自适应。新增纯相机拟合 helper 可在 Rust `comparison.rs` 用 f64 实现，不必让 renderer 跨项目持有完整模型。

### 异步接口与缓存

```text
POST /api/v1/library/comparisons {left_id,right_id,refresh?:boolean}
→ ComparisonStatus
GET  /api/v1/library/comparisons/:comparison_id
→ ComparisonStatus
GET  /api/v1/library/comparisons/:comparison_id/:side/:hash.png
→ immutable PNG
```

`ComparisonStatus` 为 `{ok,id,state:'queued'|'rendering'|'ready'|'failed'|'missing',left:{project_id,revision,url},right:{project_id,revision,url},camera,recipe,stale,reason}`，尚未确定字段用 null。服务端再次校验两个不同 ID、同一非空船 key 和源可用性，不只依赖 UI。`comparison.rs` 复用现有 scoped CDP/图片校验模式，独立有界串行队列最多8项、内存状态最多32项；同pair重试/轮询不重复启动浏览器。每项最多打开两个隔离无头页面，两阶段调用后关闭；每次 render 超时120秒，失败不泄漏浏览器。

请求线程只解析身份并排队；worker 计算两份 `SourceManifest.revision`、渲染并验证前后 revision 不变。持久缓存key涵盖两个有序项目ID、canonical源路径、两内容revision、几何hash、game build/外部资源context、web fingerprint、完整camera和recipe。先保存两PNG再原子发布descriptor；任一侧失败保留失败原因而非发布半对。状态轮询不重复hash全源；明确刷新或新compare请求重新验证。移除/重定位使原路径工作失效；内容变化标stale，不能把旧pair标当前ready。页面请求有generation和取消，最多轮询120次、间隔1秒，超时给继续刷新入口；不能无限请求后台失败任务。旧响应不能覆盖新左右选择，打开方案仍完整导航。

## 4. 扫描预览和勾选登记

重构发现阶段与登记阶段，现有 `scan(root)` 继续调用两阶段并登记所有有效项以保持 CLI/旧 API。新 UI 仅调用新接口：

```text
POST /api/v1/library/scan/preview {root}
→ {ok,scan_id,root,candidates:[{candidate_id,path,project,
     status:'new'|'registered'|'invalid',existing_id,warning}],
   warnings,visited_dirs,truncated,expires_unix}
POST /api/v1/library/scan/import {scan_id,candidate_ids}
→ {ok,entries,already_registered,warnings}
```

预览是只读发现：不调用 `add_entry`、不更新打开时间、不写目录库。现有扫描界限保持 10,000 目录、32 层、1,000 项目；无效 manifest 作为不可勾选候选或明确逐路径警告返回。发现合法项目后不进入资产；不跟链接、不扫盘根。

短期扫描 token 存服务端，最多 16 个，10 分钟过期；关联规范根目录、候选 canonical 路径、manifest 内容 hash 和已登记身份。UI 默认不勾选，已登记项展示现有 ID 且不重复创建。只提交候选 ID，服务器不接受任意新路径代替已预览候选。

导入重新检查选中候选路径、根目录包含关系、链接状态、manifest hash、合法性和当前库；任何选中项变化/失效或未知候选都返回 409 `scan_changed`，本批不写库，要求重新预览。原本 new 项若被另一个窗口登记，则返回现有 ID，不能生成副本。登记持库锁一次原子写入；只写所选有效项。旧失效记录即使名称相同也不自动重定位；用户通过现有 relocation 明确保留 ID。扫描截断始终可见，用户可登记当前已选有效项并缩小根重扫。

## 5. 模拟诊断和真实能力

集中生成每座炮的状态，开火判定和 UI 使用同一结果，避免 UI 可用但实际过滤掉所有炮。建议纯函数合同放 `engine/sim.ts`：

```ts
type FireBlockReason = 'simulation_off' | 'data_missing' | 'combat_off'
  | 'no_target' | 'outside_sector' | 'aiming' | 'reloading';
interface FireReadiness { ready: boolean; reasons: FireBlockReason[]; reload_s: number }
function mountReadiness(m: Mount, context: {
  simOn: boolean; combat: boolean; hasTarget: boolean
}): FireReadiness;
```

`data_missing` 包含缺真实绑定或炮口；详细说明保留缺失项，不能靠 synthetic muzzle 通过普通开火。每座炮显示主要原因，详情列出所有并存原因及装填秒数；无目标与正在瞄准分开。主/副炮组按钮按至少一座 `ready` 禁用/启用，禁用仍有可聚焦文字解释。保持现在开火前 `stepMounts(0)` 同步校验和 40ms 排队行为。

舵控制旁明确“舵角用于航行模拟；当前模型未驱动独立舵叶”。本轮不制造不存在的舵骨和 AA 动作。LOD 以加载的真实 mesh `userData.lod` 为依据，而不是只信 metadata 数组长度。展示可用级别集合与各部件实际级别；只有 LOD 0 时说明无额外几何级别，禁用/隐藏不可用 1/2/3。稀疏级别如 `[0,2]` 不把 2 clamp 成数组下标 1；不同部件级别不齐时选择明确回退到不高于请求的最近实际级别，并显示实际级别。auto 仅在真实多级数据存在时有意义，不因不存在级别使全船消失。

## 6. 性能验证与一项实际优化

继承前一轮性能设计；当前 closure_perf 的测量只是输入。记录服务冷启动、首次几何生成、已缓存模型启动、静态完整材质首帧、模拟可用时间和交互帧时间，分开命名，不拿单个 API 时间当用户首屏。

在实际船隔离副本、固定 build、同一 browser/GPU、同一 canvas 物理尺寸与 DPR、high 画质、相同 deck 相机/灯光下至少 3 次可比样本。报告原始值、中位数、离散范围、资源字节/材质数/mesh 数与截图。固定采样窗口排除首帧预热；GPU 不可用或浏览器退回软件时明确标记，不和 native GPU 混合。

本轮初测指向纹理请求阶段：高画质静态首屏104张PNG、约77.0MiB；服务PNG内存冷/热的TTFB P95约208.4/3.1ms，请求波仍约4.256/3.046秒；最后响应到最后decode约50–60ms，再到ready约24ms；sim接口低于2ms。GLB约14MB、传输36–74ms，但下载结束到meshes约670ms。这些是同机初测，不代替完整报告和后续可比采样。

源码 `store.loadRevs` 当前无条件读取 base/head 并在 B 后调用 A 的 applyManifest。因此优先评估 single 模式不预载不可见 A，split/side/overlay 切换时明确加载 A 并在就绪前保留完整单图/加载状态；切回 single 不丢失已缓存素材，历史revision/base变化和异步代次仍要正确。候选二是按资源scope/hash/参数隔离的跨进程PNG磁盘缓存。不要再次把已经很快的sim读取当主瓶颈。实施者以完整closure_perf报告确认最终选项。

必须重新量测同阶段并检查正确性、图像和真实蒙皮。缓存必须有明确输入失效/错误重试语义；不把降 DPR/贴图/材质/几何当作同画质加速。无改善的实验不写成完成，应继续选择有意义瓶颈或如实报告未达成。

## 7. 制作检查点和安全恢复

### 存储与创建

检查点由专用模块管理，独立于旧 edits 时间线。存储在 `WOWSMOD_HOME/checkpoints/<安全项目键>/`，内容对象按 SHA-256 去重；清单、记录 schema 化且原子发布。清单绑定 `project_id`、`checkpoint_id`、`label`、`created_unix`、`SourceManifest`、项目制作 build、捕获时目标 build、渲染版本和预览状态。创建时读取源，写对象并验证每个 hash；最后再确认源清单未变化，才发布完整记录。对象缺失、损坏或读失败时禁止宣称成功。

预览图必须来自这个被冻结的 revision。优先在 home 下物化隔离快照副本后使用现有 renderer 固定 deck 出图；不要稍后抓当前项目封面冒充历史图。合法、经验证完全匹配 revision 的已有图可以直接复制为不可变对象。生成失败保留可浏览/可恢复快照，显示“快照已保存，预览不可用”的具体原因，不制造 placeholder 成功图。

```text
GET  /api/v1/library/projects/:id/checkpoints
→ {ok,checkpoints:[Checkpoint],scope_version,excluded_roots}
POST /api/v1/library/projects/:id/checkpoints
{label,expected_revision}
→ {ok,checkpoint}
GET  /api/v1/library/projects/:id/checkpoints/:checkpoint_id/diff
→ {ok,checkpoint_revision,current_revision,
   added:[path],removed:[path],changed:[path],unchanged_count}
POST /api/v1/library/projects/:id/checkpoints/:checkpoint_id/restore
{destination,expected_revision}
→ {ok,entry,checkpoint_id,revision,url,warnings}
GET  /api/v1/library/projects/:id/checkpoints/:checkpoint_id/preview/:hash.png
→ immutable PNG
```

`Checkpoint` 至少有 `id,label,created_unix,revision,scope_version,file_count,total_bytes,project_build,target_build,preview:{state:'pending'|'ready'|'failed',url,reason}`。创建请求可用 120 秒超时；慢出图应后台完成，不阻塞已经落盘快照；只允许同时一项源捕获/物化工作，避免无界并行占用。列表无需打开模型，原项目目录暂时缺失时仍可浏览/恢复历史；只有当前 diff 和新建需要源可用。

### 差异与恢复

差异以 checkpoint 为基线：`added` 为当前新增、`removed` 为当前缺失、`changed` 为同路径 hash 不同。文件列表可分页或可折叠，但计数必须完整，不能截断后伪装无变化。没有模型也能创建、看清单和恢复。

“恢复为新方案”是本轮完整恢复操作：目标为用户指定、尚不存在的新目录，`expected_revision` 必须等于被选检查点 revision。服务端验证清单路径无绝对路径、`..`、ADS、非法/冲突路径；重新校验所有对象 hash；目标父目录不得含重解析点，不能位于原源目录、checkpoint store 或已选游戏目录内。先在同父目录的唯一 staging 写出清单文件，校验完整 manifest 和 `Project::open`，再以不覆盖方式 rename 到目标；最后通过 `LibraryStore.add` 产生新 ID。

源项目、其他项目和客户端不变；不恢复 `.install`，新项目未应用，不继承原项目的实机记录。登记失败时保留完整可用目录并明确“文件恢复成功，登记失败”，提供现有添加项目入口；不删除用户已得到的完整恢复副本。写文件或校验失败清理本次 staging，不留下看似完成的方案。重复提交在目录已存在时返回冲突，不覆写。重定位保留原 ID 历史，复制项目新 ID 不获得其他项目 checkpoint。

History 保留旧步骤浏览，增加独立检查点区，允许创建、查看固定图/修订/build/范围/差异、恢复为新方案。旧 `steps/:id/revert|restore` 未接通对象仓库，仍明确不可用，不能给它们设置 true 来代表新检查点功能。产品文案直接说明边界，而不是展示一个没有实现的“恢复”按钮。

## 8. WebGPU 的交付判断

本轮交付基于源码、官方文档和本机能力/性能证据的可选后端评估，不自动迁移默认 renderer。现有 r147 的 `onBeforeCompile` 舰体材质、ShaderMaterial 海面/特效、WebGLRenderTarget 和 UnrealBloomPass 加自定义合成链均需要单独处理；本项目没有实例化 EffectComposer，不要把它误写成现有实现。当前官方指南说明自定义 shader/旧 EffectComposer 后处理不能直接用于 WebGPURenderer，Material 文档也限定 onBeforeCompile 为 WebGL；据此结合本地代码判断现有手工后处理链需要迁移验证。它提供 WebGL 2 fallback，但不能由此推断旧 shader 可兼容，性能也并非始终优于 WebGL。[Three.js WebGPURenderer 指南](https://threejs.org/manual/pages/webgpurenderer.html)，[Material 文档](https://threejs.org/docs/pages/Material.html)。

最小评估边界是隔离实验入口/依赖：同一真实 GLB、相同原始 textures 和 rig，先对照基础 PBS/normal/MG/AO/发光、透明与蒙皮，再对照海面、bloom、tone mapping 和动态效果。以确切版本记录 adapter/support、初始化、材质编译、帧时间和固定图；仅能显示白模/标准材质的原型不得算兼容通过。若没有实现原型，结论必须写“源码兼容性评估＋本机能力探测，尚无 WebGPU 画质/性能对照”，不能写迁移完成。

是否进入默认后端迁移取决于材质/图像/rig 全部保真和实际收益。更接近游戏画面首先由贴图通道解释、着色模型、光照环境、色调映射和效果决定；API 更换本身不提供这些数据。根据现有兼容障碍，默认路径继续 WebGL，作为本轮明确的技术决策，不把未经验证的重写隐藏在完成项里。

## 验收与职责

每项至少有相应的自动回归和必要的可见 UI 验收。最终使用隔离同船异色方案检查对照、扫描选择、修订/build 记录、文件恢复、无目标/越界/装填/缺 rig，以及只有 LOD 0 的模型。测试检查原项目和假客户端的前后内容 hash 不变。

统一入口由单一集成人负责：`preview/http.rs`、`preview/library.rs`、`main.rs`、`preview/mod.rs`、`lib/api.ts`、`lib/types.ts`、`lib/i18n.ts`、`styles/app.css` 以及共享页面挂载。模块所有者在新文件独立实现后交棒；涉及同一文件的垂直切片顺序实施。任务台账和验证门由 root 按 agentdash 技能更新/展示，不从本文推导已完成状态。

最终证据保存在 `out/workbench-completion-20261114/`，稳定合同更新到 `docs/preview-library-api.md`，需求状态逐条更新。交付包含重建嵌入前端、release、重启本地服务时保留库与新元数据、右侧浏览器验收，以及按主任务已有授权完成版本交付；只有有实际证据的项标完成。
