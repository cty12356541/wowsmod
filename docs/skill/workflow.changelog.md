# workflow.changelog: skill 修订记录
上级: workflow
下级: 无
---
每轮: 日期/层/差距/根因/补法/效果。更早行见 `docs/design-changelog-archive.md`。

| 日期 | 层 | 差在哪 | 根因 | 怎么补 | 效果 |
|---|---|---|---|---|---|
| 10-12 r41 | 3 | T3/P5 侧裙远距仍偏平、无真烘焙 AO/法线、分件不够立体 | 面板仅图集弱 AO+浅倒角; import 写扁 _n/_ao; 几何无凸框/铆钉 | T4(P5.1 浮翼底): 更大倒角/更深缝/外凸框+几何铆钉; 图集加深 AO+生成 prop_n/prop_ao 并 tex import 进壳; 金/发光短缝克制; 回转/仰角/邻炮 0 | 见 p5_1/turret_T4 并排; 碰撞 0 |
| 10-12 r40d | 7 | 金/peri/水/外带件仍偏离港口; 夜无同设计掩码 | 全局曲线不够; 无真 SSAO | fit_extras 重拟合 contactAO0.45/goldHue14/periHue6/waterOpa1.45; P5.1 三联; night 粗对黑桃城 | studio15.6→game6.87; gold33→12 peri14→4.7; 真SSAO仍无 |
| 10-12 r39 | 6 | 航迹想跟主题换色 | foam*/foam_wide 实拆为数据贴图(R0/G 遮罩/B.5), 无 RGB | skill+MODDABILITY 标颜色不可改; 主题 wake enable_if 永不启用; 训练房只拍对照 | 航迹不改色 |
| 10-12 r34 | 3/5/7 | ①炮塔壳港口读成平涂盒子+后下黑缝 ②发光读成白漆 ③艏翼与舯翼风格不一 ④原壳小突起打碎色块 | ①整圈套壳+单像素纯色 UV, 缝隙被港口 SSAO 压黑; 工作台环境光强/无 SSAO 没暴露 ②发光色近白 #B8E8FF、power 0.35 ③艏翼另写厚刃 blade() ④从未盘点原壳 | ①T3: 中上带局部板/封缝/去前板/面板细节区/顶板点缀; wysiwyg 记残差交预览 ②#58B8FF + power 0.7 ③wing_fan 复用 plume, check 加共享造型语言规则 ④`model prune` + 新节点 shape.refine.clean; apply_p4 末尾 verify_apply 闸门 | T3 回转/仰角 0 碰撞; 港口见报告 |
| 10-12 r38 | 3/5 | P6 抬白把 peri 改青灰+浅 navy → 发闷; 用户否决大肩甲, 要在 P5 上微调 | 把「抬白」做成「降辅色饱和」 | P5.1: 回悬浮+LS0.88; peri 回 #A8C2F2; navy 细靴仍饱和 #243A78; violet 更清 #8B70E0; 发光线/羽轴/power 略提(0.82); 面板保持深 AO | 预览白≈38%/发光 3.7%; 港口见报告 |
