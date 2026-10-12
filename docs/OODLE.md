# Oodle 后端 / Oodle backend

本仓库**不包含**任何 Oodle 实现、DLL 或其移植源码。

## 默认：运行时加载本机 DLL（Windows）
`wowsmod` 启动后，在第一次需要解压时按以下顺序查找 `oo2core*.dll` / `oodle*.dll`，并通过 `libloading` 调用 `OodleLZ_Decompress`：
1. 环境变量 `WOWSMOD_OODLE`：DLL 文件路径，或 DLL 所在目录；
2. `wowsmod.exe` 所在目录；
3. 游戏根目录（`--game`、`WOWSMOD_GAME` 或自动探测），向下查找 3 层。

`wowsmod doctor` 的 `oodle` 字段会列出候选文件、实际加载的文件或错误原因。

**已知情况**：当前莱服客户端把 Oodle 静态链接在 `Korabli64.exe` 中，游戏目录里通常没有独立 DLL，自动查找可能找不到。这时请把 `WOWSMOD_OODLE` 指向你合法拥有的 `oo2core_*_win64.dll`（来自你已安装的其他软件），并且不要再分发它。

非 Windows 平台：可以用 `WOWSMOD_OODLE` 指向 `liboo2core*.so`，否则 Oodle 条目无法解压。

## 自定义后端
实现 `wowsmod_formats::compression::OodleBackend`，并调用 `set_oodle_backend` 注册（该注册优先于运行时加载）。`oozextract` 是 GPL-3.0 `powzix/ooz` 的 Rust 移植：如果自行接入，构建产物受 GPL 约束，不能以 MIT 再分发。

English: Oodle is loaded at runtime from a DLL already on your machine (`WOWSMOD_OODLE` → exe dir → game dir). The current Lesta client links Oodle statically, so you may need to set `WOWSMOD_OODLE` to a copy you legally own. Nothing is shipped.
