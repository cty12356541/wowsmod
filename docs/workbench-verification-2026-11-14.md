# 制作工作台验证记录 · 2026-11-14

本轮自 `f094f31145ac23019efbb96de71a7845b706d5ee` 扩展制作工作台，覆盖[需求完成表](requirements-status.md)中的原工作台优化候选和本轮审计补项。用户排除 Mod 本体美术升级。[设计](superpowers/specs/2026-11-14-workbench-completion-design.md)与[实施计划](superpowers/plans/2026-11-14-workbench-completion.md)记录验收合同。

## 构建和自动回归

本轮 Windows release 已通过构建与自动回归。下述本机证据路径相对 `out/workbench-completion-20261114/`，代码与过程报告保留在工作树。

- Rust 工作区套件 258 项通过，0 失败。五个非空套件计数为 196、53、6、1、2。复跑命令为 `cargo test --workspace --features oozextract`，日志为 `workspace-final-test.log`。
- 在 `preview_app/web` 执行 `npm test`，131 项通过，0 失败。TypeScript 检查与 Vite 构建成功；`scripts/build_preview.ps1 -Check` 核对 3 个嵌入文件。源码指纹见[嵌入构建清单](../crates/wowsmod/preview_web/build-info.json)。日志为 `frontend-final-test.log`、`frontend-final-build.log`。
- `cargo build --release --features oozextract` 完成；日志为 `release-final-build.log`。最终 `release-r5/wowsmod.exe` 的 SHA-256 为 `37f36bcd35626d613c16b973338fabfefe47a2936b2af9dec924233340b4070b`，前端源码指纹为 `3b16e5b2182a1998648de45557c341ed530931c12a22838df80b60712f16a8d3`。隔离最终服务的三份 HTTP 静态资源 SHA 与嵌入清单一致。
- 构建不匹配预检测试使用项目构建 122、匹配构建 123 和未填写构建三种情况。仅 122 出现不匹配警告，三种 dry-run 均保持隔离项目和假客户端的文件内容及登记状态。测试位于 [m4cmd.rs](../crates/wowsmod/src/m4cmd.rs)。

## 独立代码审查

- 最近修改、源修订和兼容记录审查通过。复核了目录与清单校验、列表查找边界、Windows 锁释放重试及历史读写同时发生时的原子发布。
- 选择扫描和模拟诊断审查通过。复核了扫描提交前的批量重验、真实炮口与开火判定、稀疏 LOD 和部件回退。修复并复测比较加载中旧请求提前报告新修订就绪的问题。
- 方案对照和检查点审查通过。复核了重启后继续生成冻结预览、视觉或动画源变化导致的派生几何失效，以及预期冲突和缺图的 HTTP 状态。
- 实际 Chromium 将相机整数写为 `34` 而非 `34.0`，暴露了 JSON 表示误判。现按严格数值相机校验，接受等价整数表示，仍拒绝一 ULP 差异、缺字段和非法值。新增回归先失败后通过，独立相机审查与真实双图再验均通过。
- 实际 UI 发现的检查点/对照 Escape 归焦和 390 px 恢复登记失败文字挤压均已修复复验。窄屏工作台改为纵向面板和可横向滚动的视口工具栏，宽屏保留横向三栏，resize 不重建 Viewer。
- WebGPU 评估完成。本机 Edge 获取了 RTX 5060 Ti 对应的非 fallback adapter 并创建 device。默认后端维持 WebGL，未来迁移需要同材质、相机、蒙皮和动态效果的跨后端验证。见 [WebGPU 渲染后端评估](webgpu-assessment-2026-11-14.md)。

本机审查依据位于 `.superpowers/sdd/2026-11-14-workbench-completion/` 的 `core-review.md`、`interaction-review.md`、`snapshot-comparison-review.md` 和对应实施报告。证据目录 `out/` 和过程报告 `.superpowers/` 保留在本机。

## 数据、实际出图与恢复

所有写操作都在独立 HOME、源副本及恢复目录完成。原 Robin 源 578 个文件、两个真实客户端构建的 `res_mods` 内容与 mtime 前后逐项相同；未向游戏应用、卸载或写入。隔离 QA 填写的观察记录不代表新的游戏内结论。证据见 `integration/report.md` 及其所链 JSON、PNG、脚本。

- 只读扫描不改库，选 A 仅登记 A，重复提交返回原 ID；扫描后改清单，两次旧 token 提交均为 409。改源后实机记录仍可查但与当前修订失配，旧 revision 提交为 409。
- 两份真实副本的包围盒与贴图不同。最终双图均为 640×360、DPR 1、high、day、studio、LOD 0、t=0；pos/tgt/up/fov/near/far/aspect 精确相同。同大小同 mtime 改 B 贴图，B 新图约 4,232 像素变化，A 与原图 URL 内容不变。
- 检查点冻结 575 个普通源文件、124,715,251 字节。源修改或临时缺失后，历史 PNG 字节不变。差异正确列出各一项新增、缺失和改动。普通恢复、缺源恢复、登记锁失败恢复均逐项还原 575 个 SHA-256；新 ID 无安装、实机观察或检查点继承。
- 已有目标、源内目标、检查点存储内目标返回 409。落盘后登记失败时，HTTP 与 UI 保留完整可重登记路径；解锁后正常添加成功。恢复方案实际以 high 加载 91 mesh、50 skinned mesh。损坏对象、路径穿越和 reparse 等边界由自动回归覆盖。
- 实际阻断 A 独有 27 张 PNG，分屏错误保留完整 B；解除后点 Retry，27 个请求均 200 后进入分屏。另一次阻断 B 的 36 张 PNG 得到首屏错误，实际 Retry 后恢复完整 B。

实际渲染改动后核对 HTTP 静态资产 SHA、新运行时指纹的实际双图、精确相机与恢复方案。r4 对照双图与 r3 字节相同，历史冻结图仍可读；不是只验证旧缓存图。r5 仅调整窄屏底栏，另验最终 HTTP 资产、health 与实际模型下载，保留 r4 GPU 结果。

## 可见界面与模拟

独立 Edge 用 1600、700、390 × 900、DPR 1 验证库、过滤、最近修改、兼容详情、扫描、方案对照和检查点，包含 Tab/Space/Escape、归焦、错误与重新操作。五张方案卡按源修改时间排序。扫描初始零选择，坏候选/已登记项禁用，重新扫描清空选择。对照实际换边、取消旧请求、图片失败后刷新均通过。报告为 `visual/acceptance.md`。

真实船通过正常控件依次显示未战斗、无目标、瞄准/射界外、就绪、烟焰和装填；14 座主副炮角度变化。当前模型只有 LOD 0，自动选项禁用，18 个逐件结果均为 0，独立舵叶能力边界可见。窄屏 resize 前后 canvas 身份与模型请求数不变；390/700 三面板纵向、1600 横向，视口工具栏可滚动并实际打开末端“显示”菜单。最终 r5 在三宽均完整显示画质按钮，390 下正常鼠标切中→低→高、键盘逐项归焦通过。

## 性能与边界

Windows 11、Ryzen 5 9600X、RTX 5060 Ti D3D11、Edge 147；旧/新各三次交错冷浏览器导航，模型与 ship metadata 已预生成，Windows 文件缓存保留。固定图/分屏/稳态采用相同数值机位、928×780 画布、DPR 1、high。材质清单、骨骼与实际返回 GLB 相同。自然装配 GLB 的 BIN 完全相同，JSON 仅包围盒双精度尾数差；正式采样通过两版都支持的自备模型入口载入同一 GLB，没有伪造来源标记。

- 初始 PNG 从 104 个、80,743,953 B 降为 77 个、56,135,620 B，减少 27 个请求和 **30.48% 字节**。首次分屏补齐这 27 个，随后复切零新增 PNG。
- 材质可用中位 5.231→4.837 秒，双 rAF 5.239→5.022 秒，模拟就绪 5.252→5.022 秒。服务侦听 504→806 ms，首次分屏 223→583 ms，复切 6.6→5.9 ms。页面改善不足以证明从进程启动到首帧的总时长改善。
- 稳态每版三组审阅/模拟约 164 FPS、rAF 间隔 P95 6.2 ms，接近当前调度上限。新版审阅一组同步 GPU 读回 P95 为 7 ms，不能声称渲染加速或排除小幅回归。第三组审阅同步 P95 2.2→2.5 ms，模拟 3.0→3.0 ms。
- 额外一对冷导航在任何测试 `setView` 前，记录 viewer 初见、几何就绪、材质就绪、双 rAF、模拟就绪五时点：新旧未舍入相机位置/目标/四元数/FOV/aspect/zoom/mode 全相等。此对双 rAF 时间 5.122→5.204 秒，独立保留，不混入原六轮中位。
- 原三对加补证的一对单图 PNG 均相同。原两对分屏相同，另一对有 30/723,840 个 RGB 像素不同、最大通道差 12/255；补证再现相同差异。原因未确定，实际看图未见缺纹理。部分阶段约 1.9–2.0 秒间隙保留在原始数据，不断言原因。

六轮测量针对 r1（SHA `a3b68d612df9087edf09e0b1822b431fcd7860c3a6407b23c4d036565777a768`）。后续相机数值校验、焦点、文字及窄屏布局修复不改桌面材质加载/渲染路径，但不能把六轮标成最终程序的新测。详细原始数据、统计与范围见 `performance/report.md`、`performance/paired-final/`；证据的独立复算见 `final-evidence-review.md`。

## 本机与远端交付

最终程序已运行于 8973。发布前备份整个 `wowsmod_data`、旧程序和启动信息。`delivery-verification.json` 记录三份 HTTP 嵌入资产的精确 SHA、实际 GLB、非 stale 的 640×360 封面，目录库文件 SHA 与备份相同。原源资产与安装历史、两真实客户端的 `res_mods` 内容/mtime 保持一致；打开新版预览正常重建了四份 `.wowsmod/cache/geometry.*` 派生产物，不能把交付后的整目录称为字节不变。

后台启动命令被执行策略拒绝后，改用受工具管理、可直接读取日志的长期进程会话运行预览。重启入口 `Start-Workbench.cmd` 固定 `E:\Korabli` 与端口 8973。Codex 右侧打开请求返回 queued；这是面板排队状态，没有用它代替实际浏览器验收。

本轮追加提交发布到远端 `main` 与 `codex/mod-library`，未重写历史。最终提交、两分支实查、运行身份、完整备份位置及验证引用记录于 `out/delivery/mod-library-20261113/release-info.json`。本机完整证据目录保留，QA 自有服务与浏览器已关闭，只保留正式工作台服务。
