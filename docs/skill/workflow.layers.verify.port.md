# workflow.layers.verify.port: 港口实证步骤
上级: workflow.layers.verify
下级: 无
---
**目标**: 游戏内港口同角度证据(昼+夜), 不进战斗/训练房。**完成标准**: 舷侧/艏斜/艉斜/人物近景各一张, 夜景一组, `installed --verify` 完好, 游戏已关, 用户港口设置已还原。
1. 游戏关闭 → `wowsmod install <项目>`(`--dry-run` 先看)→ `wowsmod installed --verify`。
2. 截图与操作用 `python scripts/port/port_shot.py`(Windows): `launch --exe <游戏>\<启动 exe>` → 等 60–90 s 进港 → `shot a.png --thumb a.jpg`(看缩略图决定下一步)→ `click X Y`(先在截图上量坐标)/ `orbit DX DY`(右键拖动转镜头)/ `zoom N` → 再 `shot`。鼠标移到空处(`click --right`)免得浮出提示框。
3. 夜景: 港口时段是港口自带的, 不随机。头像右侧下拉 → 「选择港口」→ 选缩略图是夜景/暗色的港口 → 「保存修改」→ 等 20 s 加载 → 拍同一组角度。**记下用户原来的港口, 拍完改回去并保存**。部分港口有调色(如黑白城市), 评审时注明。
4. 日志: `profile/python.log` 有 `registerShipMod … done`, 无新错误。动画物件拍 2 张相隔数秒的截图核对转速/方向。
5. `port_shot.py close` 关游戏 → `wowsmod uninstall <项目>`; 只有用户要求保留时才留在 res_mods, 并在汇报写明。
6. 亮度客观量: 截图里船体区域 ≥250 的像素占比与 p99 写进评审(>2% 且集中在船上算过曝)。
