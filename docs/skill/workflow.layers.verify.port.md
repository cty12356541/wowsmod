# workflow.layers.verify.port: 港口实证步骤
上级: workflow.layers.verify
下级: 无
---
**目标**: 游戏内港口同角度证据(昼+夜), 不进战斗/训练房。**完成标准**: 舷侧/艏斜/艉斜/人物近景各一张, 夜景一组, `installed --verify` 完好, 游戏已关, 用户港口设置已还原。
1. 游戏关闭 → `wowsmod install <项目>`(`--dry-run` 先看)→ `wowsmod installed --verify`。
2. 截图与操作用 `python scripts/port/port_shot.py`(Windows): `launch --exe <游戏>\\<启动 exe>` → 等 60–90 s 进港 → `shot a.png --thumb a.jpg --clean`(按住右键时游戏会隐藏港口界面, 截图无 UI)→ 换角度 `orbit DX 0 --shot b.png`(拖动中截, 同样无 UI)/ `zoom N`。`--thumb` 缩略图给 AI 看。
3. 夜景: 港口时段是港口自带的, 不随机。先 `shot --thumb` 记下当前港口(用户默认), 再 `port <港口名>`(`ports` 列出名字与夜景港口; 固化的「头像下拉 → 选择港口 → 格子 → 保存修改」)→ 拍同一组角度 → **`port <用户原港口>` 还原**。坐标按 16:9、界面 100% 标定, 不同时先看缩略图核对。调色港口(如黑白城市)评审时注明。
4. 日志: `profile/python.log` 有 `registerShipMod … done`, 无新错误。动画物件拍 2 张相隔数秒的截图核对转速/方向。
5. `port_shot.py close` 关游戏 → `wowsmod uninstall <项目>`; 只有用户要求保留时才留在 res_mods, 并在汇报写明。
6. 亮度客观量: 截图里船体区域 ≥250 的像素占比与 p99 写进评审(>2% 且集中在船上算过曝)。
