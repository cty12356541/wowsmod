# workflow.changelog: skill 修订记录
上级: workflow
下级: 无
---

每轮写: 日期 / 层 / 和参考差在哪 / 根因(分析不足·工具缺能力·执行偏差·验收太松) / 怎么补(规则或命令) / 补完效果。通用结论同步升级到对应层的规则或完成标准。

旧记录: `docs/design-changelog-archive.md`。

| 日期 | 层 | 差在哪 | 根因 | 怎么补 | 效果 |
|---|---|---|---|---|---|
| 10-11 r4 拆原包 | 1/3/4/5 | 拆流萤原包时没有现成步骤, 要靠临时脚本逐项查; 也没有"原包常见挂法/发光强度"的知识 | 工具缺能力 + 分析不足 | 新增 `scripts/ref/pack_teardown.py`(文件增改缺/visual 渲染集与 LOD/材质标识/mfm 着色器/贴图尺寸格式 mip/_mg 通道/动画 ssm·seq/脚本/垃圾文件); 层 1 加"拆原包"10 问; 层 3 加人物写进分段 visual、动画 .model→.ssm→.seq、不删 LOD; 层 4 加"官方 UV + 必须带 mip"; 层 5 加"大面积微光 B≈0.1–0.2"与 doubleSided/alphaTest | 流萤 TEARDOWN.md; 发现缺能力: PMX→glb、多材质导入、模型动画 |
| 10-11 r5 | 全部 | r4 提交后 `cargo test` 失败(本主题超 2500 字), 当时只跑了 build | 验收太松 | 每次提交前跑全量 `cargo test`; 旧行移到 `docs/design-changelog-archive.md` | 测试恢复通过 |
| 10-11 r5 | 3 | 多材质模型(PMX/GLB)只能整网格一个材质; 没有 PMX 路径 | 工具缺能力 | `model import` 按 glTF 材质拆 mfm, 贴图按内容去重, 带 doubleSided/alphaTest/自发光; JPEG/BMP/TGA 解码; `model pmx2glb`(纯 Rust); `--height` 定尺寸; 可选 Blender+mmd_tools 脚本 | Khronos AlphaBlendModeTest(CC-BY 4.0): 6 材质→6 mfm, 共用图集 1 份; 合成 PMX 通过。工作台/港口待 Windows |
| 10-11 r5 | 5 | 大面积微光只能靠 intensity 换算, 不直观 | 工具缺能力 | `glow --b-max` / `model import --b-max`: 满值处 B 上限 | 单测: b_max 40 → 满值 40、半值 20 |
| 10-11 r5 | 1/2 | 翼+光环样例被评丑; 主 skill 混入了审美判断 | 分析不足(没有风格依据就加装饰) | 主 skill 只留通用底线(比例/不穿模/远近昼夜可读/发光适度); 审美放可选风格包 `styles/<包>/STYLE.md`(配色/密度/发光/模型尺度/自评/正反例, 例图只在本机); 设计书 §2 加「采用的风格包」; 样例包 anime-flat-itasha、cyber-neon | 两个包的数值都来自实测(流萤拆解、r2 覆盖率) |
| 10-11 r5 | 3 | 会动的物件(环绕剑/自转标志)做不了 | 工具缺能力 | `model animate`(spin/orbit/float→.seq/.ssm/.model 挂钩) + `model anim-sample`; 装配/工作台按 .seq 采样播放 | 解析参考 seq 正常; 环绕半径/角速度采样正确; 港口待 Windows |
| 10-11 r6 | 全部 | 说明是平铺的 24 个主题, 入口要一次看全表; 模式与层的内容重复; 没有复盘与审美评审环节 | 引导缺失 | 重组为技能树(root→workflow 模式/层→子主题; ref 命令参考; styles 可选子树), 每节点头部写上级/下级+何时加载; `wowsmod help --tree/--children/--check`, MCP 同步; 校验无孤儿/无死链/≤2000 字; 新增 workflow.review(陌生 AI 视角复盘+审美评审+追溯到节点)、map.preview(三方向粗样做法)、verify.port | 41 节点校验通过, 最大节点 1936 字 |
