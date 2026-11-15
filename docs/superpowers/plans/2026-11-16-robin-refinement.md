# Robin 原纹理清理与局部精修 Implementation Plan

> **For agentic workers:** follow the approved user scope and this plan; each owned asset change must carry its own verification evidence. No worker installs to the real game or rewrites the original source.

**Goal:** 交付原纹理明显更干净、主炮局部更精细、可同机位比较的独立方案。

**Architecture:** 以现版 DDS/geometry 为底稿，在冻结源恢复的 variant 上做补丁式制作。纹理与几何分别在 `texture-stage` 和 `geometry-stage` 制作，避免资产及编辑记录相互覆盖；root 按各自文件路径与散列清单将资源补丁合入 `project`，负责出图、工作台状态与交付。

**Tech Stack:** 现有 wowsmod CLI / checkpoint API、Blender 5.2.2、Python 贴图数据处理、现有 DDS 导入/分卷工具。

**Spec:** `docs/superpowers/specs/2026-11-16-robin-refinement-design.md`

## Tasks

- [x] 核对原源、客户端、安装状态；保存内容 revision 与冻结检查点；恢复到新方案。
- [x] 对当前 DDS 制作保护色边/星纹/金边遮罩，按原版亮度残差清理原底纹；记录处理范围、前后图、保护区域差异。
- [x] 清理已涂装面的 detail 叠加与原 normal 细纹；保护未设计修改的 B/A 语义、压缩格式、mip、分卷和无意修改边界。
- [x] 按当前客户端真实 shader 的 normal B 天气门控，制作 B-only 涂装净化样本并进港 A/B，消除第一轮实机剩余的水渍状高光。扩域后 57 级 mip 独立复审通过，最终全范围港口复查单独收口。
- [x] 修复工作台真实 MG 通道/legacy 解释及材质诊断的 Bloom 污染，验证后重新生成同一后端的对照图；不改全局外观参数。
- [x] 导出当前 FGM049，仅修改炮裙顶盖/肩带的倒角及接缝；原位写回；验证 yaw 权重、四级 LOD、几何引用和碰撞。
- [x] 在固定日/夜近远镜头核对颜色与发光；按当前遮罩处理确实存在的溢出或失衡，不改全局预览参数掩盖问题。共同存在的夜预览炮盖夹白与未复现该问题的游戏实拍分别记录。
- [x] 精修已由用户确认的 MidBack `rb_funnel` 环冠、下方烟囱及支撑，保持原材质分区与装配空间；表面纹理与材质另由纹理步骤完成。
- [x] 生成同机位旧/新图，实际操作瞄准/回转/抬炮/齐射，复核无新增穿模与纹理异常；反馈问题回对应 owner 修复。主炮烟、装填与副炮白焰已观察，主炮闪焰首帧和完整后坐行程未逐帧捕获。
- [x] 记录源资源、变更与证据，在制作库提供原版和精修版，并打开精修版供审阅。最终黑桃城夜港与海洋日港通过，已恢复测试前客户端状态；冻结 575 个源文件、默认封面与实机记录就绪，`.wsmod` 安装包已生成。验证边界见[交付记录](../../robin-refinement-2026-11-16.md)。

## Verification boundaries

美术更改不用镜像实现的单元测试替代视觉判断。必要数据检查包括：DDS 格式/大小/mip/分卷、预览完整材质、结构和权重、保护区域、原源散列与客户端未意外改写。只检查受改动影响的工作台能力，不重新运行无关工具链全套。

几何验证已覆盖目标两件的 LOD 0–3、14 座炮位与原节点/权重，以及 95,997 个真实骨骼姿态采样；含全周 yaw 补扫，不把禁射区当机械止挡。独立审查已闭合该项。仍保留离散采样、未覆盖后坐与真实游戏逻辑的边界。

## Production ownership

- `robin_texture_refine`：`scripts/model/robin/refine_textures.py`、`texture-stage` 的 DDS/MFM、`out/robin-refine-20261116/textures/` 证据；不修改 geometry/visual。
- `robin_geometry_refine`：`scripts/model/robin/refine_geometry.py`、`geometry-stage` 的目标 geometry/visual、`out/robin-refine-20261116/geometry/` 证据；不修改 DDS/MFM。
- root：最终 variant、整合清单、日夜近远和动作验收、完成记录；原源只读。
- `robin_preview_material`：工作台 MG 材质解释、非外观诊断通道的后处理隔离及直接相关验证；不修改模型资产或外观预设的光照/色调参数。
