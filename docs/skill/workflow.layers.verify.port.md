# workflow.layers.verify.port: 港口实证步骤
上级: workflow.layers.verify
下级: 无
---
1. 游戏关闭 → `wowsmod install <项目>`(`--dry-run` 先看)→ `wowsmod installed --verify`。
2. 启动游戏进港(只进港口, 不进战斗; 训练房需授权)→ 选目标船 → 固定机位截图: 舷侧、艏斜、艉斜、近景人物/道具、远景; 夜间港口时段随机, 多进几次或换港口。
3. 日志: `profile/python.log` 有 `registerShipMod … done`, 无新错误。
4. 动画物件: 录 2 张相隔数秒的截图或短视频, 核对转速/方向。
5. 关闭游戏 → `wowsmod uninstall <项目>`; 只有用户要求保留时才留在 res_mods, 并在汇报写明。
