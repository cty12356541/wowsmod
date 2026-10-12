# wowsmod

非官方的《战舰世界》莱服（Lesta / Мир кораблей）Mod 制作工具包：Rust 命令行 `wowsmod` + 内嵌 React 预览前端，用于读取本机游戏资源、制作/预览/打包涂装与模型类 Mod。

## 快速开始（Windows）
1. 在 GitHub Releases 下载 `wowsmod-<版本>-windows-x64.zip`，解压到任意目录。
2. 在该目录打开终端，运行 `wowsmod doctor` 检查环境。
3. 未自动找到游戏时：`wowsmod doctor --game <游戏根目录>`，或设置环境变量 `WOWSMOD_GAME=<游戏根目录>`。
4. 使用 AI 工具（Cursor、Claude Code 等）时，让它先读 `AGENTS.md`。

**需要你自备**
- 正版、已安装的莱服客户端。
- Oodle 运行库：会自动从你本机查找（`wowsmod doctor` 显示结果）；找不到时设置 `WOWSMOD_OODLE`，见 `docs/OODLE.md`。
- （可选）Blender，部分建模功能需要。

压缩包内容：`wowsmod.exe`（已内嵌预览前端）、`README.md`、`AGENTS.md`、`LICENSE`、`NOTICE`、`THIRD_PARTY_LICENSES.md`、`docs/`、`styles/`。

## AI 工作流搭建 / AI setup

本项目以 AI 代理驱动为主：AI 通过 MCP 或命令行调用 `wowsmod`，并从 `AGENTS.md` 开始逐层阅读技能树。

1. **安装 wowsmod**：下载 Release 里的 zip 并解压（或从源码构建），把 `wowsmod.exe` 所在目录加入 PATH。
2. **自检**：运行 `wowsmod doctor`，确认找到了游戏；Oodle 和 Blender 如有缺失，见「需要自行下载的组件」。
3. **初始化 AI 工作目录**：运行 `wowsmod ai init <工作目录>`。它会写入：
   - `AGENTS.md`（正本）；
   - 指向正本的 `CLAUDE.md`、`.cursor/rules/wowsmod.mdc`、`GEMINI.md`、`.github/copilot-instructions.md`；
   - MCP 配置 `.cursor/mcp.json` 和 `.mcp.json`。
4. **注册 MCP**：用 Cursor 或 Claude Code 打开该目录，配置会自动生效；其他客户端见下方配置示例。
5. **（可选）安装 Blender**：只有改外形（shape）的流程需要，见下方 Blender 说明。
6. **让 AI 开始**：例如对它说「读 AGENTS.md，用 wowsmod 新建一个 <船名> 的 SDK 项目，把船体改成红色」。AI 会依次执行：
   - `wowsmod_status`
   - `wowsmod_help`
   - `mod init … --mode sdk`
   - 改色等操作
7. **第一张图**：调用 `wowsmod_render`（MCP 直接返回 PNG），或手动运行 `wowsmod render <项目> --compare`。
   - 需要 Edge 或 Chrome；没有浏览器时加 `--engine soft`。
   - 首次冷启动需要十几秒到几十秒，之后每张约 1 秒。
8. **安装进游戏**：先运行 `wowsmod install <项目或包>`，再进港口查看；用 `wowsmod uninstall <名>` 可完全还原。

### (a) wowsmod 自带 MCP 服务器
- 命令：`wowsmod mcp`（stdio 方式，无参数）。
- 启动后暴露 5 个入口工具：
  - `wowsmod_status`：查看游戏、已装 mod、项目状态；
  - `wowsmod_help`：读取技能树文档；
  - `wowsmod_run`：执行任意命令（自动加 `--json`）；
  - `wowsmod_render`：出图，直接返回 PNG；
  - `wowsmod_expand`：按需展开分组工具，分组有 `texture`、`model`、`project`、`install`、`assets`。
- 展开分组后，服务器会发送 `notifications/tools/list_changed`。客户端不支持这个通知时，一直用 `wowsmod_run` 即可。
- 详见 `wowsmod help ref.mcp`。

Cursor（`.cursor/mcp.json`）、Claude Code（`.mcp.json`）、Claude Desktop（`%APPDATA%\Claude\claude_desktop_config.json`）的配置格式相同：
```json
{
  "mcpServers": {
    "wowsmod": { "command": "C:\\tools\\wowsmod\\wowsmod.exe", "args": ["mcp"],
                 "env": { "WOWSMOD_GAME": "<游戏根目录>" } }
  }
}
```
Codex（`~/.codex/config.toml`）：
```toml
[mcp_servers.wowsmod]
command = "C:\\tools\\wowsmod\\wowsmod.exe"
args = ["mcp"]
```
其他 MCP 客户端：命令填 `wowsmod`，参数填 `mcp`，传输方式选 stdio。不支持 MCP 的工具，直接在终端运行 `wowsmod … --json` 即可。

### (b) Blender
- **工作流不使用 Blender MCP。** 技能树和脚本只用两种方式：
  - 命令行往返：`wowsmod model export-blender` / `import-blender`；
  - 无头脚本：`blender --background --python scripts/blender/*.py`。
- 官方下载：https://www.blender.org/download/ ，许可证 GPL-2.0-or-later。它作为独立程序运行，不链接进本项目，因此不影响本项目的 MIT 许可。
- 安装后由 `wowsmod doctor` 自动探测；也可以设置环境变量 `WOWSMOD_BLENDER=<blender.exe 路径>`。
- 需要 Blender 的技能节点：
  - `workflow.layers.shape*`（包括 `shape.blender` 和 `shape.refine.*`）；
  - `ref.blender`、`ref.model.import`；
  - `learn.makers.extra-model`。
- 没有 Blender 时：贴图、改色、发光、出图、打包、安装都不受影响。外形修改可以在 Blender 里手动编辑后，只运行 `import-blender`。
- 如果想让 AI 实时操作 Blender，可以自行安装第三方的 blender-mcp 插件（不属于本项目，也未经测试）。

### (c) 其他 AI / 工具依赖
- 技能树中没有引用 wowsmod 以外的其他 MCP。
- 出图依赖本机 Edge 或 Chrome（通过 CDP 调用），可用 `WOWSMOD_BROWSER` 指定路径。
- `scripts/` 下的辅助脚本需要 Python 3 或 Node。

## 需要自行下载的组件 / Optional external components

以下组件因许可证或版权原因**不随本项目分发**，需要时请自行获取。`wowsmod doctor` 检测到缺失时会提示你查看本节。

| 组件 | 用途 | 许可证 | 官方来源 | 放置 / 检测方式 | 没有它时 |
|---|---|---|---|---|---|
| 正版莱服客户端 | 资源来源、安装目标 | Lesta EULA（专有） | https://lesta.ru | 自动探测；或 `--game <根目录>` / `WOWSMOD_GAME` | 无法工作 |
| Oodle 运行库（oo2core_*_win64.dll） | 解压 Oodle 压缩的游戏资源 | RAD/Epic 专有，**不随本项目分发** | 你自己已安装的软件；官方：https://www.radgametools.com/oodle.htm | 自动查找顺序：`WOWSMOD_OODLE`（DLL 文件或目录）→ `wowsmod.exe` 同目录 → 游戏目录（3 层内）；通过 `libloading` 调用 `OodleLZ_Decompress`；`wowsmod doctor` 会显示找到/缺失 | 只能处理未压缩/Deflate 条目、本地项目、打包、预览；大多数游戏贴图和模型无法读取 |
| oozextract / ooz（仅开发者备选） | 没有 Oodle DLL 时，自行编译解码后端 | crates.io 标注 MIT，但是 GPL-3.0 ooz 的移植，按 GPL 对待 | https://github.com/lvlvllvlvllvlvl/oozextract | 自行 fork 本项目、添加依赖并注册后端；构建产物不能以 MIT 再分发 | 同上 |
| pfsunpack | 仅用于开发期逐字节对照验证 | 专有 / 社区工具 | WG/Lesta 官方 ModSDK 相关发布 | 运行时不需要；打包时会自动排除 | 一切正常 |
| Blender（可选） | 模型导入导出、渲染预览 | GPL-2.0+（独立程序，不链接进本项目） | https://www.blender.org/download/ | 自动探测；或 `WOWSMOD_BLENDER=<blender.exe>` | 模型编辑/渲染相关命令不可用，其他命令正常 |
| Chrome / Edge（可选） | 预览截图（CDP） | 各自许可 | https://www.google.com/chrome/ | 自动探测；或 `WOWSMOD_BROWSER=<路径>` | 无法自动截图，交互预览仍可用 |
| Python 3（可选） | `scripts/` 下的辅助脚本 | PSF | https://www.python.org/ | 放在 PATH 中 | 只是辅助脚本不可用 |
| 参考模型 / 图片（如 Robin/知更鸟、Krishna） | 主题设计参考 | 原作者条款（多数禁止再分发） | 原发布页 | 放在你本地的项目目录，**不要提交** | 不影响工具本身 |

English: none of the items above ship with wowsmod. The game and an Oodle backend are required to read most game assets; everything else is optional. GPL or proprietary components must be obtained by you and must not be redistributed together with MIT builds.

## 从源码安装
1. 安装 Rust stable（`rustup`）。
2. `git clone <repo> && cd wowsmod`
3. `cargo build --release`（默认**不含** Oodle 解码器，见下）
4. 二进制位于 `target/release/wowsmod`。

### 运行时需用户自备
- 正版、已安装的莱服客户端（资源从你本机的游戏目录读取）。
- Oodle 运行库：wowsmod 会自动在游戏目录和 exe 同目录查找 `oo2core*.dll`；**注意：当前莱服客户端把 Oodle 静态链接在主程序里，游戏目录中通常没有独立 DLL**。这种情况下请把 `WOWSMOD_OODLE` 指向你合法拥有的 `oo2core_*_win64.dll`（例如其他已安装的游戏或引擎自带的副本），详见「需要自行下载的组件」。
- Blender（可选，部分建模脚本使用）。
- 参考模型/参考图（如 Robin/知更鸟、Krishna 示例）：仅以文字说明引用，**仓库不含任何素材**，请自行从原作者处按其许可获取，且不得再分发。

## 入口
AI 代理/自动化工作流请从 `AGENTS.md` 开始阅读；人工使用见 `docs/`。

## 免责声明
本项目为非官方作品，与 Lesta Games、Wargaming 无任何关联，亦未获其认可。使用者必须合法拥有游戏。本仓库**不包含任何游戏资源**（模型、贴图、PFS/idx 等）及第三方模型素材。使用 Mod 请遵守游戏 EULA，风险自负。

## 许可
MIT，见 `LICENSE`；第三方声明见 `NOTICE` 与 `THIRD_PARTY_LICENSES.md`。

---

## English summary
`wowsmod` is an **unofficial** modding toolkit for Lesta's *World of Warships* (Мир кораблей): a Rust CLI plus an embedded React preview UI. Quickstart: download the Windows zip from Releases, unzip, run `wowsmod doctor` (use `--game <dir>` or `WOWSMOD_GAME` if needed), and point AI tools at `AGENTS.md`. You must own the game. Oodle is loaded at runtime from an `oo2core*.dll` already on your machine (auto-searched in the game dir / next to the exe, or set `WOWSMOD_OODLE`); the current Lesta client links Oodle statically, so you may need to point `WOWSMOD_OODLE` at a copy from other software you legally own. No DLL is shipped. Or build with `cargo build --release`. Start at `AGENTS.md`. Not affiliated with or endorsed by Lesta Games or Wargaming. You must own the game; **no game assets, Oodle binaries or third-party models are included** — they are read/fetched from your own installation at runtime. MIT licensed.
