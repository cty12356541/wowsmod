# Third-Party Licenses — wowsmod

Generated 2026-10-10 (Asia/Shanghai). Scope: Rust release dependency graph of `wowsmod` (Windows target) + `preview_app/web` build-time npm tree (embedded frontend is the built `preview_web/` assets).

## Summary

| Layer | Count | Notes |
|---|---|---|
| Rust crates (resolved normal deps) | 129 | `cargo tree` / `cargo deny check licenses` → **licenses ok** under MIT/Apache/BSD/Zlib/0BSD/Unlicense/MPL-2.0/Unicode-3.0 |
| npm packages (preview_app/web node_modules) | 126 | MIT/ISC/Apache/BSD/0BSD; one CC-BY-4.0 (`caniuse-lite`, build-time only) |
| Oodle | 0 | not included; see docs/OODLE.md |

## Rust crates by license family

### 0BSD / MIT / Apache-2.0

- adler2 2.0.1

### BSD-2-Clause / Apache-2.0 / MIT

- zerocopy 0.8.62
- zerocopy-derive 0.8.62

### MIT

- block_compression 0.10.0
- bytes 1.12.1
- generic-array 0.14.7
- meshopt-rs 0.1.2
- mime_guess 2.0.5
- rust-embed 8.13.0
- rust-embed-impl 8.13.0
- rust-embed-utils 8.13.0
- simd-adler32 0.3.10
- slab 0.4.12
- strsim 0.11.1
- winnow 0.7.15
- winreg 0.55.0
- wowsmod 0.1.0
- wowsmod-formats 0.1.0
- zmij 1.0.23

### MIT OR Apache-2.0

- anstream 1.0.0
- anstyle 1.0.14
- anstyle-parse 1.0.0
- anstyle-query 1.1.5
- anstyle-wincon 3.0.11
- anyhow 1.0.104
- ascii 1.1.0
- bitflags 1.3.2
- block-buffer 0.10.4
- block-buffer 0.12.1
- bstr 1.13.1
- cfg-if 1.0.5
- chunked_transfer 1.5.0
- clap 4.6.7
- clap_builder 4.6.7
- clap_derive 4.6.7
- clap_lex 1.1.1
- colorchoice 1.0.5
- const-oid 0.10.2
- cpufeatures 0.2.17
- cpufeatures 0.3.1
- crc32fast 1.5.2
- crossbeam-deque 0.8.8
- crossbeam-epoch 0.9.21
- crossbeam-utils 0.8.23
- crypto-common 0.1.7
- crypto-common 0.2.2
- digest 0.10.7
- digest 0.11.3
- dirs 6.0.0
- dirs-sys 0.5.0
- either 1.19.0
- env_filter 2.0.0
- env_logger 0.11.11
- equivalent 1.0.2
- fdeflate 0.3.7
- flate2 1.1.10
- futures 0.3.34
- futures-channel 0.3.34
- futures-core 0.3.34
- futures-executor 0.3.34
- futures-io 0.3.34
- futures-macro 0.3.34
- futures-sink 0.3.34
- futures-task 0.3.34
- futures-util 0.3.34
- half 2.7.1
- hashbrown 0.17.1
- heck 0.5.0
- httpdate 1.0.3
- hybrid-array 0.4.15
- indexmap 2.14.2
- is_terminal_polyfill 1.70.2
- itoa 1.0.18
- libc 0.2.190
- log 0.4.34
- mime 0.3.17
- once_cell_polyfill 1.70.2
- pin-project-lite 0.2.17
- png 0.17.16
- proc-macro2 1.0.107
- quote 1.0.47
- rayon 1.12.0
- rayon-core 1.13.0
- regex 1.13.1
- regex-automata 0.4.18
- regex-syntax 0.8.11
- serde 1.0.229
- serde_core 1.0.229
- serde_derive 1.0.229
- serde_json 1.0.151
- serde_spanned 0.6.9
- sha2 0.10.9
- sha2 0.11.0
- shellexpand 3.1.2
- syn 2.0.119
- syn 3.0.6
- test-log 0.2.21
- test-log-core 0.2.21
- test-log-macros 0.2.21
- thiserror 2.0.21
- thiserror-impl 2.0.21
- tiny_http 0.12.0
- toml 0.8.23
- toml_datetime 0.6.11
- toml_edit 0.22.27
- toml_write 0.1.2
- typenum 1.20.1
- unicase 2.10.0
- utf8parse 0.2.2
- windows-link 0.2.1
- windows-sys 0.59.0
- windows-sys 0.61.2
- windows-targets 0.52.6
- windows_x86_64_msvc 0.52.6

### MPL-2.0

- option-ext 0.2.0

### Unicode-3.0 (+MIT/Apache)

- unicode-ident 1.0.26

### Unlicense / MIT

- aho-corasick 1.1.5
- globset 0.4.20
- memchr 2.8.3
- same-file 1.0.6
- walkdir 2.5.0
- winapi-util 0.1.11

### Zlib / Apache-2.0 / MIT

- bytemuck 1.25.2
- bytemuck_derive 1.12.1
- miniz_oxide 0.8.9
- miniz_oxide 0.9.1
- safe_arch 0.7.4
- wide 0.7.33
- zune-core 0.4.12
- zune-jpeg 0.4.21

## npm packages by license (preview frontend build)

Direct runtime deps: `@radix-ui/react-dialog`, `@radix-ui/react-dropdown-menu`, `@radix-ui/react-slider`, `@radix-ui/react-tabs`, `@radix-ui/react-toggle-group`, `@radix-ui/react-tooltip`, `@types/react`, `@types/react-dom`, `@types/three`, `@vitejs/plugin-react`, `lucide-react`, `react`, `react-dom`, `react-resizable-panels`, `three`, `typescript`, `vite`, `zustand`

### 0BSD (1)

- tslib

### Apache-2.0 (2)

- baseline-browser-mapping
- typescript

### BSD-3-Clause (1)

- source-map-js

### CC-BY-4.0 (1)

- caniuse-lite

### ISC (6)

- electron-to-chromium
- lru-cache
- lucide-react
- picocolors
- semver
- yallist

### MIT (115)

- @babel/code-frame
- @babel/compat-data
- @babel/core
- @babel/generator
- @babel/helper-compilation-targets
- @babel/helper-globals
- @babel/helper-module-imports
- @babel/helper-module-transforms
- @babel/helper-plugin-utils
- @babel/helper-string-parser
- @babel/helper-validator-identifier
- @babel/helper-validator-option
- @babel/helpers
- @babel/parser
- @babel/plugin-transform-react-jsx-self
- @babel/plugin-transform-react-jsx-source
- @babel/template
- @babel/traverse
- @babel/types
- @esbuild/linux-x64
- @floating-ui/core
- @floating-ui/dom
- @floating-ui/react-dom
- @floating-ui/utils
- @jridgewell/gen-mapping
- @jridgewell/remapping
- @jridgewell/resolve-uri
- @jridgewell/sourcemap-codec
- @jridgewell/trace-mapping
- @radix-ui/number
- @radix-ui/primitive
- @radix-ui/react-arrow
- @radix-ui/react-collection
- @radix-ui/react-compose-refs
- @radix-ui/react-context
- @radix-ui/react-dialog
- @radix-ui/react-direction
- @radix-ui/react-dismissable-layer
- @radix-ui/react-dropdown-menu
- @radix-ui/react-focus-guards
- @radix-ui/react-focus-scope
- @radix-ui/react-id
- @radix-ui/react-menu
- @radix-ui/react-popper
- @radix-ui/react-portal
- @radix-ui/react-presence
- @radix-ui/react-primitive
- @radix-ui/react-roving-focus
- @radix-ui/react-slider
- @radix-ui/react-slot
- @radix-ui/react-tabs
- @radix-ui/react-toggle
- @radix-ui/react-toggle-group
- @radix-ui/react-tooltip
- @radix-ui/react-use-callback-ref
- @radix-ui/react-use-controllable-state
- @radix-ui/react-use-effect-event
- @radix-ui/react-use-is-hydrated
- @radix-ui/react-use-layout-effect
- @radix-ui/react-use-previous
- @radix-ui/react-use-rect
- @radix-ui/react-use-size
- @radix-ui/react-visually-hidden
- @radix-ui/rect
- @rolldown/pluginutils
- @rollup/rollup-linux-x64-gnu
- @rollup/rollup-linux-x64-musl
- @types/babel__core
- @types/babel__generator
- @types/babel__template
- @types/babel__traverse
- @types/estree
- @types/prop-types
- @types/react
- @types/react-dom
- @types/three
- @types/webxr
- @vitejs/plugin-react
- aria-hidden
- browserslist
- … and 35 more

## Notable crate notes

- `option-ext` MPL-2.0 (weak copyleft; linking as dependency of `winreg` is generally fine for MIT distribution if notices preserved).

- `unicode-ident` adds Unicode-3.0 data license alongside MIT/Apache.

- `meshopt-rs` MIT — Rust reimplementation of meshoptimizer (MIT upstream).

- `block_compression` MIT — BC1–7 codecs.

- `miniz_oxide` MIT/Zlib/Apache — deflate.

- `zune-jpeg` / `zune-core` MIT OR Apache-2.0 OR Zlib — JPEG 解码(model import 多材质 GLB 贴图), 纯 Rust。

- 外部可选工具(不分发, 不链接): Blender(GPL-2.0+)与 mmd_tools 插件(GPL-3.0), 仅由 `scripts/model/pmx_to_glb_blender.py` 在用户机器上命令行调用。

- 测试模型不入库: `scripts/model/fetch_test_models.sh` 下载 Khronos glTF-Sample-Assets 的 AlphaBlendModeTest(© 2018 Analytical Graphics, Inc., CC-BY-4.0, 作者 Ed Mackey)到本地。
