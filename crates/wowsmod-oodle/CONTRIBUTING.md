# 向 crates/wowsmod-oodle 贡献

- 这是洁净室 crate（MIT，`forbid(unsafe_code)`）。**任何解码逻辑**的提交者，都必须先在 `docs/oodle-cleanroom/ALLOWED_SOURCES.md` 的声明表中签名。
- 只能依据 `HANDOFF.md` §2 列出的材料；§3 列出的材料一律禁止阅读。意外接触到时，在声明表里如实登记，并且不要再负责相关模块。
- 不要提交游戏数据（`.blk` / `.gold` / 语料），也不要提交从任何二进制中提取的表或常量。
- 小步提交；提交说明里写清依据（SPEC 章节或自己的实验）。
- 提交前请运行 `cargo test -p wowsmod-oodle`（有语料时设置 `OODLE_CORPUS`）。
