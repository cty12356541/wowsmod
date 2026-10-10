# workflow.layers.fx: 层 6 特效(全局贴图)
上级: workflow.layers
下级: 无
---
**目标**: 评估炮口火焰/开火烟/烟囱烟/曳光是否随主题。
**能改的**: 粒子/尾迹贴图 `particles/animated/*`、`trails/*`(`wowsmod find` → `extract` → `tex recolor`/替换 → `tex import --like`)。参数编译在 `content/assets.bin`, 不改。
**风险**: 贴图全局共享, 影响所有船; 只做纯视觉, 不碰玩法数值。
**验证**: 烟囱烟港口可见; 开火类需训练房, 未经用户明确授权不进, 否则写"未实证"。
**完成标准**: 列出被影响的全局贴图并告知用户; 能验证的已截图。
