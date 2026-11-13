# workflow.layers.verify.wysiwyg: 预览近似与实机差异
上级: workflow.layers.verify
下级: 无
---
**用途**：工作台用于快速比较与定位问题；game look 是历史港口拟合，不能保证所见即所得。
`wowsmod render <项目> --look game-day|game-night|studio`：game-day/night 用于近似昼夜，studio 用于展示与形状。图中注明 look、机位、距离、修订；不要把三种光照混作同一证据。
**流程**：同机位小样对比 → 昼夜/远近检查 → 按授权港口复核 → 把差异写进项目。自定义部件明暗、细发光线、金色/白色/紫色、水面与阴影都可能有残差；未知构建/港口不能默认沿用旧拟合结论。
**对齐机位**：`--query "view=custom&az=<度>&el=<度>&dist=<单位>&fov=<度>&tgt=x,y,z"`；无法严格对齐时记录误差。
**需要重新校准时**：读 `scripts/preview/calib/CALIBRATION.md`；新实拍和像素区写入独立 spec，保留旧校准，用 fit/eval 与既有案例回归后再考虑工具改动。校准是单独的工具任务，不为了一个 Mod 静默重写全局 look。
**证据边界**：历史校准未实现真实 SSAO，也缺完整夜景分区色差证据；r40–r42 数值只描述 P2/P5.1 样本。原始参数、残差与来源见 `workflow.changelog` 案例归档。完成当前预览检查不等于港口验收通过。
