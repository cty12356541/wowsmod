# wowsmod 计划书与验收单

> 版本 v0.1 · 2026-10-10 · 工作区 `<WORK_DIR>`
> 现有成果(TOOLKIT.md / tools/ / preview/)作为参考与素材保留,不删除、不覆盖。

## 一、目标

做一个**开源**工具包,让任何玩家用**任意一个 AI 编程工具**(Cursor / Claude Code / Codex / ZCode 等)
加上这个工具包,就能用一句话描述完成莱服(Мир кораблей / Lesta,26.x)舰船 mod 的制作:
从解包、改贴图/发光、**改模型**,到预览、打包、装进游戏、卸载,全程不用懂底层格式。

## 二、需求(已确认)

| # | 需求 | 说明 |
|---|---|---|
| R1 | 面向他人分发 | 不懂技术的玩家也能用;不写死任何本机路径 |
| R2 | 开源 | 放 GitHub;第三方组件按许可证决定打包或首次运行时下载 |
| R3 | 不绑定 AI 工具 | 通用接入:命令行 + `AGENTS.md` 说明书 + 可选 MCP 服务 |
| R4 | 无运行时依赖 | 放弃 Python;用 Rust 做成单文件 `wowsmod.exe` |
| R5 | 第一版支持改模型 | 不止换贴图/发光,要能把改过的模型写回游戏 |
| R6 | 可视化预览 | 内置、重做;重点是让使用者随时看见"改了什么",可撤回 |
| R7 | Blender 可选 | 预览与简单改模不依赖 Blender;只给精修用 |

## 三、架构

```
┌────────────────────────────────────────────────────┐
│ 上层  AI 接入: AGENTS.md(+各工具规则副本)  wowsmod mcp │
├────────────────────────────────────────────────────┤
│ 中层  工作流: project / edit / preview / pack / install│
├────────────────────────────────────────────────────┤
│ 底层  格式库(Rust): idx/pkg 解包 · geometry 读写 ·    │
│       DDS 读写 · assets.bin · GameParams · PnF 结构   │
└────────────────────────────────────────────────────┘
          单文件 wowsmod.exe(预览网页资源内嵌)
```

命令草案:
`wowsmod doctor`(自动找游戏、查版本与环境)· `find` · `extract` · `export` · `edit` ·
`preview` · `pack` · `install` / `uninstall` · `history` / `undo` · `mcp`

一个 mod 就是一个项目文件夹:`mod.toml`(元信息)+ `edits/`(改动记录)+ `assets/`(改后的资源)。
AI 只改项目文件夹,由 wowsmod 负责生成游戏格式,保证可回退、可重放。

## 四、里程碑

### M0 探路验证(最高优先,决定 R5 能否成立)
- 拆 1~2 个社区成品莱服 mod,弄清游戏实际接受的文件结构
- 用官方皮肤(FSB016 Royal / FSB017 Repub)对照,弄清皮肤的组织方式
- 最小改模型实验:黎塞留一个炮塔整体放大 2 倍 → 写回 `.geometry` → 装进游戏看港口
- 最小贴图实验:黎塞留主贴图改色 → 装进游戏看港口
- 第三方组件许可证排查(pfsunpack/MKUT、wows-toolkit、texconv、three.js、Blender)

### M1 格式库与核心命令
- Rust 工程搭建,复用 wows-toolkit 的可用代码
- 解包:莱服 26.x 新压缩(自研或合法集成 pfsunpack)
- `geometry → GLB` 导出,内置顶点池/索引配对修复(取代 glb_repair.py)
- `doctor` 自动定位游戏目录与版本,`find` / `extract` / `export`

### M2 贴图与发光
- DDS 读写(BC1/BC5/BC7 + mip,含 .dd0~.dd2 分卷)
- 编辑操作:改色、套图、发光遮罩(_mg)绘制/生成

### M3 模型写回
- `GLB → geometry` 编码器(莱服格式)
- 内置简单改模:缩放 / 移动 / 旋转 / 替换部件 / 删除部件
- Blender 往返(可选):导出 → Blender 精修 → 导回

### M4 打包与安装
- 生成 PnF 结构(或 M0 验证出的正确结构)
- `install` / `uninstall`,游戏更新后 `migrate` 到新 bin 目录
- 与汉化等其他 mod 共存检查

### M5 可视化预览(重做)
- `wowsmod preview` 启动内嵌网页,浏览器自动打开
- 模型:改前/改后叠加或并排,改动部位高亮
- 贴图:A/B 滑条,发光单独显示
- 每步改动自动刷新 + 改动记录列表 + 一键撤回

### M6 AI 接入
- `AGENTS.md`:任务套路、命令说明、常见坑(从 TOOLKIT.md 提炼)
- 规则副本:`.cursor/rules`、`CLAUDE.md` 等指向 AGENTS.md
- `wowsmod mcp`:把命令暴露为 MCP 工具

### M7 开源发布
- GitHub 仓库、许可证、README(中/英/俄)
- CI 自动构建 Windows 单文件
- 附一个示例 mod 项目 + 新手教程

## 五、风险

| 风险 | 影响 | 应对 |
|---|---|---|
| 莱服不接受 WG 式 PnF 结构(包内无 .visual/.model/.mfm) | R5 可能受阻 | M0 先拆成品 mod 与官方皮肤找真实结构 |
| geometry 写回格式有未知字段 | 写回后游戏崩或不显示 | 先做"读→原样写→字节比对"往返测试 |
| pfsunpack 不可再分发或不开源 | 无法打包 | 首次运行下载;长期自研解压 |
| 游戏更新改格式 | 工具失效 | `doctor` 检测版本;格式库做版本分支 |
| 游戏规则/封号风险 | 用户受损 | 只做本地外观类 mod,文档写明;不碰战斗数据 |

## 六、验收单

**M0 探路**
- [ ] 产出《莱服 mod 结构说明》,注明依据(成品 mod / 官方皮肤)
- [ ] 改色贴图 mod 进港口可见,附截图
- [ ] 放大炮塔 mod 进港口可见,附截图(或明确结论:哪一步不行、为什么)
- [ ] 许可证清单:每个组件可否打包、处理方式

**M1 核心**
- [x] 新电脑上只放 `wowsmod.exe`,`doctor` 能自动找到游戏并报版本 (证据: wowsmod/out/logs/doctor.json; 26.10.0.0.8867689 @ D:\Korabli)
- [x] `extract` 解出的 DDS/geometry 与 pfsunpack 结果逐字节一致 (accept_extract 429/429; accept_textures 2000/2000 含 BC7Prep 238; 见 wowsmod/M1_REPORT.md; 19 个 Hydra 极小末块: M2 已修复, 全量 342,823 条 0 失败)
- [x] `export` 导出的黎塞留四段船体经体检 0 越界、0 尖刺三角 (accept_export: 勃艮第/黎塞留/FGM049 LOD0+全LOD 0/0; visual mapping 78/78; geometry 往返 18/18)

**M2 贴图**
- [x] 读→写 DDS 往返,游戏内显示无差异 (rt_loose: _a/_mg 重编码 + _n 去 BC7Prep, 截图 02 vs 06 无差异; 见 wowsmod/M2_REPORT.md 第 4 节)
- [x] 一句话"把黎塞留改成红色、烟囱发蓝光",AI 用 wowsmod 完成,进游戏可见 (用户没有黎塞留, 改用勃艮第: 整船深海蓝 ✔(港口实测 ΔE≈3, port-albedo 补偿) + 烟囱发蓝光 ✔(根因: .visual 材质标识选着色器, glow 已自动改; power 0.25); 截图与证据见 wowsmod/M2_REPORT.md 第 5b/6 节。未在战斗/夜间验证)

**M3 模型**
- [ ] 读→原样写 geometry,与原文件逐字节一致(或差异有解释且游戏内无差别)
- [ ] 缩放/移动/替换部件三种改动各一例,进游戏可见
- [ ] (可选)Blender 往返一例

**M4 打包安装**
- [ ] `install` 后游戏正常启动、mod 生效;`uninstall` 后完全复原
- [ ] 与已装汉化共存正常
- [ ] 模拟游戏更新(换 bin 目录)后 `migrate` 可用

**M5 预览**
- [ ] 无需 Python/Node/Blender,`preview` 一条命令打开
- [ ] 改前/改后对比、改动高亮、发光单显、改动列表、撤回 全部可用
- [ ] 58 万顶点整船流畅交互(普通笔记本 ≥30 帧)

**M6 AI 接入**
- [ ] Cursor、Claude Code、另一款工具各跑一遍同一个任务,都能独立完成
- [ ] MCP 模式下,工具列表正确、调用成功

**M7 发布**
- [ ] 仓库公开,CI 产出可下载的 exe
- [ ] 新用户照教程 30 分钟内做出第一个 mod

## 七、执行约定
- 只在 `<WORK_DIR>` 内新增文件,不动现有成果
- 往游戏 `res_mods` 安装任何测试 mod 前先征得同意
- 每个里程碑结束更新本文件的验收勾选,并附证据(截图/日志)
