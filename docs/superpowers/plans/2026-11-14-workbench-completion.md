# 制作工作台完成 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 完成已授权的工作台候选与审计漏项，交付可操作、可验证的库状态、扫描、对照、模拟诊断、检查点和性能改进，保留原 Mod。

**Architecture:** 增量扩展现有 Rust/React 工作台，复杂状态放入独立模块，所有身份以项目 ID 和内容 revision 为准。共享入口由 root 单点集成；检查点恢复到新目录形成新方案，不覆写原项目；WebGPU 以有证据的迁移决策交付。

**Tech Stack:** Rust workspace（rust-version 1.80）、tiny_http、serde/sha2、React 18、TypeScript 5、Zustand、Three.js r147、Node 内建 test runner、现有 PowerShell 构建脚本。

**Spec:** `docs/superpowers/specs/2026-11-14-workbench-completion-design.md`

## Global Constraints

- 原 Mod 和真实客户端只读；需要资产变化的验证只能使用隔离副本。
- 所有新状态按稳定 `project_id` 隔离；重定位保留历史，复制并登记取得新 ID。
- 原 `preview <path>`、`render`、MCP、CLI `library scan` 和旧 HTTP `/library/scan` 保持兼容。
- 列表不得等待 GLB 装配、图片解码、浏览器启动或全项目内容散列。
- 不跟随源目录中的符号链接、Windows junction/reparse point；不静默跳过会影响完整性的源读取错误。
- 保留模型、材质、蒙皮和高画质能力；性能结论必须使用相同机位、实际像素尺寸、DPR 和画质条件。
- 新文案覆盖中文、英文、俄文；复用现有浅灰/白/蓝视觉、键盘操作和窄屏布局。
- 已有用户授权，不设置额外方案确认门；worker 不自行部署/提交/重启用户服务。

## Review Focus

- 扫描后路径被替换、被重定位或另一个窗口已登记：只导入所选且重新验证的候选，不劫持旧 ID（Task 4）。
- 内容相同但目录变了，或内容改变但大小/mtime 相同：前者保留 revision，后者使验收记录失配（Task 2）。
- 快照对象缺失/损坏、清单路径穿越、目标已存在：恢复在写出可见目标前失败，源和目标既有文件不变（Task 7）。
- 左右对照和详情的旧异步响应晚到：不能把 A 的图/记录显示成 B（Tasks 3、5、7）。
- 部件 LOD 非连续或不齐、缺 rig 且 UI 进入战斗：展示实际能力和原因，不能消失/假开火（Task 6）。

## Ownership 与依赖波次

root owns 集成入口和最终交付；workers 不是唯一工作者，不回退他人改动。常规实现/测试使用 `gpt-6.1-sol`，疑难渲染和关键审查使用 `gpt-6-astra`，显式 `reasoning_effort="ultra"`，独立上下文提供接口/路径/授权边界。

- Wave A：Task 1 真实修改时间（完整小切片）；Task 2 源 revision/记录核心（新文件）；Task 8 性能测量和 WebGPU 资料评估可并行。
- Wave B：Task 1 释放 `library.rs`/`Library.tsx` 后 Task 4 扫描；Task 2 完成后 Task 7 检查点核心；Task 6 模拟诊断与性能改动涉及 `viewer.ts` 时必须串行交棒。
- Wave C：Task 3 兼容摘要、Task 5 对照服务/渲染和 UI、Task 7 检查点 UI 分别按新模块实现。Task 5 与 Task 7 可复用序列化 camera/render meta，但 `render.ts` 只有一个 owner。root 统一挂载 API、页面、类型、文案和样式；不要让三个 worker 并发修改 Library/WorkbenchNav/History。
- Wave D：Task 9 统一验证、独立审查、修复、嵌入产物和交付。按 agentdash 在派发/审查/修复/完成状态变化时展示真实台账。

共享文件可由某个 worker 临时取得排他 ownership，但 root 必须明确交棒；下表式的并行编辑不被默认授权。下面路径均相对仓库根，行号因同时改动不固定，按函数锚点定位。

### Task 1：真实源文件修改时间

**Files:** Modify `crates/wowsmod/src/library.rs`（`LibraryEntryView`、`LibraryStore::list`、tests），`preview_app/web/src/lib/types.ts`、`lib/library.ts`、`lib/library.test.mjs`、`components/Library.tsx`、`lib/i18n.ts`。本 task 完成后交还共享文件。

**Interfaces:** Produces `LibraryEntry.modified_unix: number|null` 与 `LibraryFilters.sort` 新值 `'modified'`；不改 catalog schema。Consumes 现有 `LibraryStore` 路径规范化和错误显示。

- [x] 添加 Rust 行为测试：`mod.toml` mtime 较早、asset 较晚，返回asset；fx.json和资产增删更新实际源时间；修改 `.wowsmod/cache`、库标签/收藏/opened 不改变结果；资产内 junction 不遍历，不能给完整时间；不存在源为 null + 警告。前端测试：`[null,100,200]` 按 `[200,100,null]`，相同时间按 ID，旧 opened 排序不变。
- [x] `cargo test -p wowsmod library::tests -- --nocapture` 与前端 `node --test src/lib/library.test.mjs` 观察新用例在实现前失败；记录实际原因。
- [x] 实现有界的 `source_modified_unix(root: &Path) -> Result<Option<u64>>`，只读 `mod.toml`、可选 `fx.json` 与 `assets/res_mods` 的普通文件/目录，接入卡片文案、持久化 sort 白名单和排序。
- [x] 重跑定向测试和 `npm run typecheck`；用隔离项目实际编辑一个 asset 后刷新显示/排序变化，打开项目和生成图不改变源修改时间。

### Task 2：共享内容 revision 与实机记录核心

**Files:** Create `crates/wowsmod/src/project_revision.rs`、`crates/wowsmod/src/workbench_records.rs`；root modifies `main.rs` module declarations。测试放各新模块的 `#[cfg(test)]`。

**Interfaces:** Consumes library stable ID 与 root path；Produces `SourceFile`、`SourceManifest`、`read_source_manifest(root: &Path) -> Result<SourceManifest>`（完整类型见 spec）。`VerificationStore::open(home: &Path) -> Result<Self>`、`list(project_id: &str) -> Result<Vec<VerificationRecord>>`、`append(project_id: &str, root: &Path, request: VerificationRequest) -> Result<VerificationRecord>`；request 字段精确对应 spec 的 POST。对 ID 的磁盘 key 使用 SHA256，不直接拼接输入。

- [x] 写源 revision 测试：同字节复制到另一路径 revision 相等；同长度内容改变且恢复 mtime 仍不等；缓存/安装备份不影响；含链接、不可读输入、路径大小写冲突、超限或源读中变化失败。快照范围覆盖 `mod.toml/assets/edits/fx/design/自定义源文件`。
- [x] 写记录测试：无记录不通过；不同 build 或 revision 不匹配；同 revision/build 最近失败覆盖旧通过；记录后源变化返回409语义且历史不改；重定位保留原 ID，复制新 ID 无历史；损坏 JSON 不覆写。
- [x] `cargo test -p wowsmod project_revision -- --nocapture`、`cargo test -p wowsmod workbench_records -- --nocapture` 观察新增测试失败，再实现规范清单、稳定散列、原子记录和匹配 helper。
- [x] 重跑上述命令。向 root 交付类型/API 签名和错误码 `source_changed`、`unsafe_source`、`snapshot_limit`，不得在库 listing 扫描全部内容字节。

### Task 3：兼容性摘要与记录 UI/API

**Files:** Create `preview_app/web/src/components/CompatibilitySummary.tsx`、`lib/compatibility.ts`、`lib/compatibility.test.mjs`；root modifies `preview/http.rs`、`preview/library.rs`、`lib/api.ts`、`lib/types.ts`、`Library.tsx`、`WorkbenchNav.tsx`、`lib/i18n.ts`、`styles/app.css`；Modify `crates/wowsmod/src/m4cmd.rs` tests（如现有 fake-client 回归位于脚本，则扩充该脚本，保持同断言）。

**Interfaces:** Consumes Task 2；Produces `libraryApi.compatibility(id)` 与 `libraryApi.recordVerification(id,request)`，精确 wire shape 见 spec。组件接收 `{entry:LibraryEntry,targetBuild:string|null,onChanged?:()=>void}`；列表摘要只读历史，详情才计算精确 revision。

- [x] 写 `compatibility.test.mjs`：安装为 installed 但无记录仍未记录；项目122/目标123显示差异；历史 passed/build122 不算目标123通过；记录中 passed 与更新 failed 选后者；A 慢响应不能覆盖 B。
- [x] 增加 fake-client apply dry-run 回归：target123/project122，断言 warning、target_build=123，以及前后项目和 client 树 hash 相等；不运行真实安装。
- [x] 先跑 `node --test src/lib/compatibility.test.mjs`、相应 Rust/脚本定向测试观察新增失败，再接入 GET/POST 路由、输入界限和详情表单；所有 HTTP 复用现有 origin/JSON 校验。
- [x] 重跑定向测试、typecheck；可见 UI 验证保存记录→重开存在→改隔离 asset→旧记录显示历史修订；构建/安装/制作状态各自独立，不写“工具已验证”。

### Task 4：只读扫描与选择登记

**Files:** Modify `crates/wowsmod/src/library.rs`；Create `crates/wowsmod/src/preview/scan.rs`、`preview_app/web/src/components/ScanImport.tsx`、`lib/library-scan.ts`、`lib/library-scan.test.mjs`；root modifies `preview/mod.rs`、`preview/library.rs`、`preview/http.rs`、`Library.tsx`、`lib/api.ts`、`lib/types.ts`、`lib/i18n.ts`、`styles/app.css`。依赖 Task 1 释放文件。

**Interfaces:** `LibraryStore::scan_preview(root:&Path) -> Result<ScanPreview>` 只发现；`LibraryStore::import_candidates(candidates:&[ScanCandidate]) -> Result<ImportResult>` 在锁内重验并一次写库；旧 `scan` 包装上述能力。`ScanSessions` 持最多16个/10分钟 token 并产生 spec 的 preview/import 路由；frontend `libraryApi.scanPreview(root)`、`scanImport(scanId,candidateIds)`。

- [x] Rust 测试夹具含正常、重复、无效、深度截断、junction；预览前后 catalog 字节相同，不创建 library.json；只选两个之一只登记一项，重复返回原 ID。
- [x] 加竞态测试：扫描后 manifest 改变/目录变链接/移出根→整批零写；扫描后别处登记→复用现有 ID；失效同名旧记录不自动 relocation；token过期/未知候选拒绝；已有 CLI scan 仍直接登记合法项。
- [x] 前端测试初始0选择、已登记/无效不可选、截断可见、重新扫描清空旧选择、导入失败保留上下文、慢扫描不回写新root。
- [x] 跑 `cargo test -p wowsmod library::tests -- --nocapture`、`cargo test -p wowsmod preview::scan -- --nocapture`、`node --test src/lib/library-scan.test.mjs` 观察失败；实现发现/提交拆分与 ScanImport，复用原扫描限额。
- [x] 定向全部通过后，浏览器勾选隔离目录候选，查看库只新增选中者；确认旧 `/scan` 响应兼容且新UI从未调用直接登记端点。

### Task 5：独立方案对照

**Files:** Create `crates/wowsmod/src/preview/comparison.rs`、`preview_app/web/src/components/CompareProjects.tsx`、`lib/project-compare.ts`、`lib/project-compare.test.mjs`、`lib/render-camera.test.mjs`；Modify `render/render.ts`；root modifies `preview/mod.rs`、`preview/library.rs`、`preview/http.rs`、`lib/api.ts`、`lib/types.ts`、`Library.tsx`、`WorkbenchNav.tsx`、`lib/i18n.ts`、`styles/app.css`。仅必要相机公开能力可改 viewer，需与Task6/8交棒；不改变旧单项目 A/B。

**Interfaces:** Consumes Task2 SourceManifest、`cdp::Page`、existing thumbnail占位与open入口；Produces spec 的 ComparisonStatus 和三条 comparison API、`CompareProjects({entries,initialLeftId,onClose})`。`RenderParams.pos`、可传入near/far、精确 `RenderResult.meta.camera/bounds` 由本task定义并供Task7复用；backend `fit_comparison_camera(left_bounds,right_bounds,reference_camera)->Result<CommonCamera>` 纯函数保证投影相同。

- [x] 写前端用例：同名不同ID分别选择；同一ID/不同船被拒；仅一个项目提示；封面占位不能显示“相机已同步”；任一侧失败保留原因；慢旧pair响应不落到新选择；轮询达到上限停止。
- [x] 写相机测试：parse精确pos/tgt/fov，NaN/Infinity/同pos和tgt报bad_param；显式相机不调用preset/focus/auto-fit；meta不舍入；union相机令两边bounds角点落在fill=.87内，同一camera应用两次所得投影相同。
- [x] 写Rust队列/缓存测试：同pair去重，8项上限，双ID/双revision/canonical路径/build/geometry/web变化分别失效；一侧失败不发布半对；render中源修改或relocation失败；两PNG与descriptor原子发布，坏hash不可读；两侧最终camera/meta不相同则拒绝ready。
- [x] 先跑 `cargo test -p wowsmod preview::comparison -- --nocapture` 与 `node --test src/lib/project-compare.test.mjs src/lib/render-camera.test.mjs` 观察新增失败；扩展render精确相机/meta，worker先探测两bounds，计算共同camera，两个scoped页面用同camera各出一张最终图，关闭所有自有浏览器。
- [x] root挂载异步API和选择器；状态请求只查任务，不每秒扫描源hash。最终图统一640×360/DPR1/high/deck方向/day/studio/LOD0/t=0；旧封面仅在加载时明确标注。
- [x] 重跑测试/typecheck。隔离两套同船不同外观/包围盒做可见验证，保存两张render meta并断言pos/tgt/up/fov/near/far/aspect以及recipe一致；1600/700/390宽度完整显示且不串项目，两个入口打开正确项目。

### Task 6：炮塔原因、舵能力和真实 LOD

**Files:** Modify `preview_app/web/src/engine/sim.ts`、`engine/viewer.ts`、`components/SimPanel.tsx`；Create `lib/simulation-readiness.test.mjs`、`lib/lod-capabilities.test.mjs`；root integrates `lib/i18n.ts` 与必要样式。和 Task 8 的 viewer 编辑串行。

**Interfaces:** Produces spec 的 `FireBlockReason`、`FireReadiness`、`mountReadiness()`；`SimSnapshot.mounts[]` 加 `readiness`，整体加 `availableLods:number[]`（来自实际 meshes）。`readyMounts` 与 UI 共用 readiness，readiness 本身不改变射击物理。

- [x] 用真实 Three 对象/已有 rig 测试夹具覆盖 sim off、缺 rig/muzzle、非战斗、无目标、射界外、俯仰未就绪、装填、全部ready，以及同时缺条件；普通开火不得借 synthetic muzzle 通过。
- [x] LOD测试：实际只有0但 metadata声明0–3，只报告0；实际[0,2]能选2；某部件仅0时全局选择2仍显示0；auto没有可用更低级别时不消失；当前选级别在模型刷新后失效则回退并同步store。
- [x] `node --test src/lib/simulation-readiness.test.mjs src/lib/lod-capabilities.test.mjs` 观察失败；集中判定，接入snapshot/组按钮/每炮原因和舵叶提示，按真实level值选择mesh。
- [x] 重跑定向测试和既有 `glb-skin/simulation-sync/viewer-fx` 回归；正常UI验证四类无法开火原因、齐射和装填，真实船14座主副炮保持可动；只有LOD0模型没有虚假等级。

### Task 7：检查点核心、差异、恢复和 UI

**Files:** Create `crates/wowsmod/src/preview/checkpoints.rs`、`preview_app/web/src/components/Checkpoints.tsx`、`lib/checkpoints.ts`、`lib/checkpoints.test.mjs`；root modifies `preview/mod.rs`、`preview/library.rs`、`preview/http.rs`、`History.tsx`、`lib/api.ts`、`lib/types.ts`、`lib/i18n.ts`、`styles/app.css`。Consumes Task 2。核心建议独立 worker，UI 可在合同固定后并行。

**Interfaces:** `CheckpointStore::open(home:&Path)->Result<Self>`、`list(project_id:&str)->Result<Vec<Checkpoint>>`、`create(project_id:&str,root:&Path,request:CreateCheckpoint)->Result<Checkpoint>`、`diff(project_id:&str,checkpoint_id:&str,root:&Path)->Result<CheckpointDiff>`、`restore(project_id:&str,checkpoint_id:&str,destination:&Path,expected_revision:&str)->Result<RestoredProject>`。service wrapper负责调用 store.add、提供 game/build 与固定图后台任务；旧步进恢复 API 不复用。HTTP shape 与字段见 spec。

- [x] 写创建/重开测试：snapshot A→改源B→重开仍得A清单/对象；mtime伪装变化仍被hash识别；同内容对象去重；源读中变化不发布；scope排除cache/install且包含fx/edits/design和普通源文件。
- [x] 写恢复安全测试：A→B差异的added/removed/changed方向；恢复新目录内容hash=A、源保持B、新ID未应用无实机历史；目标已存在、目标在源/游戏内、reparse parent、`../`/绝对路径/ADS/大小写冲突、坏hash/缺对象均零覆写；注入写失败只剩可清理staging，注册失败保留完整新目录并返回可操作结果；原源目录丢失仍可list/restore。
- [x] `cargo test -p wowsmod preview::checkpoints -- --nocapture` 观察失败；实现CAS和原子清单/捕获稳定性/安全新目录恢复，所有最终目录发布前验证 `read_source_manifest` 和 `Project::open`。
- [x] 加固定图：冻结快照物化到home隔离目录，既有renderer用deck/day/studio/LOD0出图；保存图hash和recipe/build，后台失败明确reason。测试源随后改变不改变历史图绑定，缺模型仍保存快照并显示failed图状态。不得向用户原源写入生成文件。
- [x] 前端 `checkpoints.test.mjs` 覆盖空历史、创建成功/图失败、revision变化409、重新打开读历史、差异空/非空、A/B项目切换旧响应隔离、恢复目标存在错误与登记失败结果。先失败后实现UI，History保留旧步骤浏览并增加独立检查点区。
- [x] 定向测试/typecheck通过后隔离真实项目走创建→改一份普通源/asset→对照差异→新目录恢复→打开新方案，前后按文件hash核对；截图明确旧revision/build与恢复新ID。

### Task 8：真实性能改进与 WebGPU 决策

**Files:** root 先根据 closure_perf 报告指定排他 ownership；可能是 `preview/state.rs`、`preview/library.rs`、`store.ts`、`engine/viewer.ts` 的一个阶段。证据写 `out/workbench-completion-20261114/performance/`。Create `docs/webgpu-assessment-2026-11-14.md`（实施owner，不属于本设计worker）；若采用独立spike，放 `experiments/webgpu-preview/` 并隔离依赖。

**Interfaces:** Performance report includes `commit/browser/gpu/project_revision/game_build/canvas_px/dpr/quality/camera/sample_kind/raw_samples/median/range/materials/meshes/texture_bytes`。本task不在测量前承诺固定优化算法。

- [x] 固定相机数值、high画质与物理像素尺寸，至少3次同条件记录 cold/cache/first-static/sim-ready 和稳定帧窗口；并保存模型/材质清单与基准图。确认不是软件渲染或不同DPR被当作同条件。
- [x] 优先验证 `store.loadRevs` 无条件 `applyManifest('A', mA)` 的不可见base加载是否构成104PNG/77MiB请求波主占比；single先仅完整B，切split/side/overlay再加载A并显示加载状态，base/viewRev/geometry变更保持代次隔离。更新“boot必须先完成两侧”的旧断言为single完整B、compare完整A+B；另测切compare过程中再切single、切项目、请求失败与重试。若收益不成立再评估按scope/hash/参数区分的PNG磁盘缓存，不优先动<2ms的sim接口。
- [x] 选定阶段实施后同条件重测；图像/可见材质/rig不变且阶段时间改善，已进入compare模式仍全部材质就绪，未改善继续查因，不能以降低画质结案。
- [x] 阅读本地 shader/后处理/骨骼实现与两份官方 primary source，记录本机 `navigator.gpu/requestAdapter` 结果和版本。产出兼容阻碍、画质先决条件、收益假设与迁移结论。
- [x] 如果做WebGPU原型，仅隔离真实GLB/材质/rig对照入口；不能使默认工作台依赖新版Three。未实现的效果列未评估，未做GPU对照明确无性能结论。完成条件是有依据的技术决定，不是宣称已迁移。
- [x] 关键审查核对采样公平性、图像/材质数量和风险结论；与 Task 6 共享 viewer 时先合并一个task再运行另一个。

### Task 9：系统验证、修复与实际交付

**Files:** root modifies `docs/preview-library-api.md`、`docs/requirements-status.md`；build输出 `crates/wowsmod/preview_web/**`；审计证据 `out/workbench-completion-20261114/`。其他产品文件只有审查发现的必要修复。

- [x] 确认各合同已接通，追加 shared-file diff检查，没有worker覆盖他人改动。新routes保持origin/JSON/ID校验；端点不能先无谓创建重模型session。性能不因列表新增源hash而倒退。
- [x] 运行 `cargo test --workspace --features oozextract`，前端 `npm test`、`npm run build`；受沙箱Node spawn限制时只使用明确记录的 `node --test --test-isolation=none src/lib/*.test.mjs` 替代，不隐藏失败。
- [x] 用 `scripts/build_preview.ps1` 生成嵌入资产，随后 `scripts/build_preview.ps1 -Check`；`cargo build --release -p wowsmod --features oozextract`。记录真实测试数量/结果和source fingerprint。
- [x] 独立 Astra 关键审查只检查可利用/可复现正确性：跨项目身份、源/目标写边界、源变化竞态、记录真实性、scan提交、snapshot恢复和渲染退化。修复后运行受影响验证，不机械重复所有无关测试。
- [x] computer-use agent进行真实浏览器验收：制作库筛选/修改排序、兼容详情/记录、scan选择、两方案对照、炮原因/LOD、检查点创建/差异/恢复；1600/700/390宽度与键盘。使用隔离资产和假client，真实client保持未应用状态。
- [x] 更新需求表每项实现/证据/边界；WebGPU写实际评估状态，Mod升级仍明确排除；不存在未接线按钮被标成功。最后由root按既有授权交付运行服务与版本，保留现有library以及新records/checkpoints，右侧浏览器显示最终版本。

## Self-review

spec的八项能力分别由Tasks 1、2–3、5、4、6、8、7、8覆盖，旧apply构建警告门由Task3覆盖。共享字段/路由在spec定义一次，各task使用相同命名；`expected_revision` 在create/verification表示当前源，在restore表示选中不可变checkpoint，UI需按操作取值。危险输入、异步旧响应和LOD稀疏数据均有归属测试。检查点采用恢复为新方案的边界必须出现在产品和完成表中；对照的共同数值相机由实际meta相等证明，独立fit的旧封面只作标明的占位。
