# SYNC: box ⇄ Windows 双边保存

两台机器之间只有"逐个文件拷贝"通道(单个文件 ≤ 4 MB 才稳定, 2 MB 最稳), 因此:

## 1. 代码仓库(git, 两边 HEAD 必须一致)
- Windows `D:\WoWSModding\wowsmod` 与 box `/workspace/wm/src` 是同一个 git 历史(box 的 remote `win` 来自 bundle)。
- 发送端: `git bundle create x.bundle <对端已有提交>..HEAD`(首次 `--all`) → 拷到对端暂存目录 → 接收端 `git pull --ff-only x.bundle HEAD`(或 `git fetch x.bundle main:refs/remotes/box/main` 再 merge)。
- 两边各自 `git log -1 --format=%H` 对比, 一致才算同步完成; 在汇报里写出提交号。
- 不在 git 的: 资产(素材/模型/包)、构建产物、`styles/*/examples/`。

## 2. 工作目录镜像(报告/分析/设计书/对比图/脚本): `scripts/sync/syncpack.py`
- box `/workspace/e2e` → Windows `D:\WoWSModding\e2e`。规则 `scripts/sync/e2e_rules_text.txt`(文本) 与 `e2e_rules.txt`(含 ≤5 MB 图片)。
- 发送: `python syncpack.py pack <源> <发件箱> --rules <规则> --remote <对端 .sync/manifest.json> --chunk 2`
  → 只打包对端清单里没有或哈希不同的文件; 小文件合成 tar.gz 块, 大文件切片; `plan.json` 记块与文件的 sha256。
- 搬运: 逐个拷贝 `plan.json` 与 `c*.tgz / c*.part`(失败就重试那一块)。
- 接收: `python syncpack.py check <收件箱>` 列出缺失/损坏块(续传只补这些) → `apply <收件箱> <目标>` 校验块→解包→逐文件 sha256→更新 `<目标>/.sync/manifest.json`。
- 下一轮先把对端 manifest 拷回发送端作为 `--remote`, 实现增量。

## 3. 大素材(模型包/参考包)
- 不进 git、不进镜像规则; 按 2–4 MB 切片拷贝, 接收端拼接后核对整包 sha256 再解压(压缩包文件名按 GBK 解码)。
- 位置: Windows `D:\WoWSModding\refs\<主题>\`, box `/workspace/e2e/<主题>/` 或 `/workspace/e2e/refs/`。记录见 `D:\WoWSModding\e2e\SYNC_LOG.md`。

## 4. 节奏
每个阶段结束: 代码 bundle 同步 + 两边提交号一致 + e2e 镜像增量一次, 汇报写明。
