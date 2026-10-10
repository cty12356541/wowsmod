# M1 报告(进行中 — 断点续作记录)

> 本文件在 M1 进行中持续更新, 掉线后从"进度"一节继续。最终版会替换本节。

## 进度(最新在上)

- [x] 工程: cargo workspace(wowsmod-formats + wowsmod), MSVC + crt-static 单文件 exe, 本地 git
- [x] doctor 实机: 注册表 LGC-1740622596 → D:\Korabli, 26.10.0.0.8867689, 225 idx / 342,823 条目
- [x] find: glob/关键字; --ship 基于 GameParams(1165 条舰船, 与 Python pickle 逐条一致), 俄文名可检索
- [x] BC7Prep 逆变换(纯 Rust, crates/wowsmod-formats/src/bc7prep.rs): 12 个真实样本(覆盖 flags 0x50/0x51/0x53/0x56/0x77/0x7b/0xc0,
      模式 0–7 与纯色模式 9)还原后与 pfsunpack 逐字节一致; extract 默认还原, --raw 保留原始字节。
      未被真实数据覆盖的分支: 模式 4 旋转 0、模式 7 R 端点非零 → 待 accept_textures.ps1 大样本统计。
- [~] extract vs pfsunpack: 首轮 406/429 一致(23 个 BC7Prep 贴图不一致) → 加入还原后待重跑 accept_extract.ps1
- [ ] export 验收脚本(勃艮第/黎塞留/FGM049)、visual 交叉验证、全量 geometry 往返
- [ ] M1_REPORT 定稿、PLAN.md 勾选
