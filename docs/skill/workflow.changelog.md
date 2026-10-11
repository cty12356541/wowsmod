# workflow.changelog: skill 修订记录
上级: workflow
下级: 无
---

每轮写: 日期 / 层 / 和参考差在哪 / 根因(分析不足·工具缺能力·执行偏差·验收太松) / 怎么补(规则或命令) / 补完效果。通用结论同步升级到对应层的规则或完成标准。

旧记录: `docs/design-changelog-archive.md`。

| 日期 | 层 | 差在哪 | 根因 | 怎么补 | 效果 |
|---|---|---|---|---|---|
| 10-11 r10 | 全部 | 陌生 AI 走查(假想 角色×别的船): theme/book 等缺 输入/完成标准; 无「没有 3D 模型」分支; 夜景写成「时段随机」; ref 节点与提示里写死示例船名 | 引导缺失 | 补各层 目标/输入/完成标准; theme 加无模型分支; verify.port 改成港口选择器+还原用户港口+亮度量化; 示例改占位符; AGENTS 模板加许可与 --json-out | 走查记录见 e2e 报告 |
| 10-11 r11 | 7 | 工作台夜景远暗于港口夜景, 用它判断过曝会误判 | 验收太松 | verify.port 第 6 条: 以港口截图量 ≥250 像素占比/p99 | 夜港 船体削顶 <1.3%, 未过曝, 不调 |
| 10-11 r12 | 2/3/7 | 对比图中文标签乱码; 米/单位要先导入才知道; 整船同色要逐张 recolor; 港口截图被 UI 挡、换港口靠量坐标; 网页声明的许可无处标 | 工具缺能力 | `scripts/ref/cjkfont.py`(系统中文字体, `WOWSMOD_CJK_FONT` 覆盖); `mod set --ship-length` + `model list` units; `ship-color`; port_shot `--clean`/`orbit --shot`(按住右键隐藏界面)与 `port <名字>`; `mod license --note` / `--license-note` | A3 炮塔浅色变体一条 spec 出图; 港口昼夜无 UI 截图; 港口切换+还原实测 |
| 10-11 r13 | 1/2/全部 | 陌生 AI 走查(复刻: 别船发光视频; 原创: 一句风格): 流程写「三模式都出 2~3 方向」与复刻冲突; 原创会出两轮预览; 层 1 完成标准只适合复刻; 通用底线在 workflow 与 styles 各写一份; 复刻别人 mod 的许可没提 | 引导缺失 | 复刻只出 1 个粗样; 原创的 ≥3 方向即 §4; 层 1 完成标准按模式分; 通用底线只留 workflow 一份; 复刻立项 `mod license --note`; 非 B 站视频取帧说明 | 树双向索引与可达性脚本校验: 41 节点无孤儿/死链 |
| 10-11 r14 | 3/主题 | 验证件把知更鸟全身 PMX 当雕像挂上船; skill 把「有模型就 import」当完成标准 | 引导缺失 | theme/shape/map.preview/风格包写死: 主题=设计语言→涂装+道具外带件, 禁止人物雕像; PMX 只导出道具; 走查补芙莉莲×大和反例 | 设计书 v0.2 + 三方向重做 |
| 10-11 r15 | learn | 社区流程散落站外 | 引导缺失 | 增 `learn`→`learn.makers` 五子节点(texture/extra-model/emissive-pnf/sdk-pipeline/pitfalls)+`e2e/learn/MAKERS_NOTES.md`; guide 校验含 learn | 本轮 |
| 10-11 r17 | 3 | 外带只靠 CLI `--at/--height` 易穿模/比例飘; 社区镜学家用 Blender/ZBrush 精修; 扁平立环 `--height` 按薄轴爆缩放 | 引导缺失 | 增 `workflow.layers.shape.blender`(何时进、比例/拓扑/贴合、glb 标准、验收、薄环用 `--scale`); C 上限按此流程 | C 天花板交付 |
| 10-11 r16 | 主题 | 社区高质量主题=UV重画+道具外带, 非立人像 | 引导缺失 | theme 加社区对齐句; makers 总览写死 theme 禁立人像/复刻可例外 | 与 shape/theme 一致 |

