# workflow: 做一整套外观 mod(模式判断 → 设计书 → 分层执行)
上级: root
下级:
- workflow.replicate: 有参考视频/图, 要做得像它
- workflow.theme: 用户给一个角色 + 作品
- workflow.original: 只有风格/情绪, 没有参考对象
- workflow.book: 写/改/定稿设计书(三种模式共用)
- workflow.layers: 定稿后按层执行, 或做方向预览
- workflow.review: 每层结束的复盘与审美评审
- workflow.changelog: 记录 skill 的修订(每轮都写)
---
**判断模式**: 有参考视频/图 → replicate; 角色+作品 → theme; 只有风格描述 → original。
**流程(三模式一致)**: 设计书 init → 层 1–2(模式节点说明各自的第 1 步)→ 2~3 个方向预览 → 交用户迭代 → 用户明确定稿 → 层 3–7 → 验收。每层结束做 `workflow.review`。
**通用底线(与风格无关)**: 比例协调; 不穿模; 远近昼夜可读; 发光适度。审美取向写进风格包(`wowsmod help styles`), 设计书 §2 写明采用哪个。
**出图规则**: 每层的检查图来自 `wowsmod preview` 工作台或浏览器 `render`(先 `render --warm`); `--engine soft` 只量尺寸。
**缺能力先补工具**: 清单要的命令不存在就先补或如实告诉用户, 不要降级硬凑(模型→贴图、插画→程序线条)。
**素材与版权**: 官方素材/模型只在本地用, 不进仓库; 带第三方模型的 mod 包按其规约标"不可公开分发"。
