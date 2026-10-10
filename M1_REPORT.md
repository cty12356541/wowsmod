# M1 报告(进行中 — 断点续作记录)

> 本文件在 M1 进行中持续更新, 掉线后从"进度"一节继续。最终版会替换本节。

## 进度(最新在上)

- [x] 工程: cargo workspace(wowsmod-formats + wowsmod), MSVC + crt-static 单文件 exe, 本地 git
- [x] doctor 实机: 注册表 LGC-1740622596 → D:\Korabli, 26.10.0.0.8867689, 225 idx / 342,823 条目
- [x] find: glob/关键字; --ship 基于 GameParams(1165 条舰船, 与 Python pickle 逐条一致), 俄文名可检索
- [~] extract vs pfsunpack: 首轮 406/429 一致; 23 个不一致全部是 DDS 头 dwReserved1[2]=1 的 BC7 纹理,
      游戏内为 Oodle Texture **BC7Prep** 变换后的数据(头: version, flags, mode_counts[10]; 尾部为每块 4 位的模式表)。
      wowsmod 输出的是游戏原始字节(crc32 与 idx 一致), pfsunpack 额外做了 BC7Prep 逆变换。→ 正在逆向逆变换。
- [ ] export 验收脚本(勃艮第/黎塞留/FGM049)、visual 交叉验证、全量 geometry 往返
- [ ] M1_REPORT 定稿、PLAN.md 勾选
