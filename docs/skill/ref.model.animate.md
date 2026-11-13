# ref.model.animate: 动画物件(实验能力)
上级: ref.model
下级: 无
---
```
wowsmod model animate --project <项目> --section <分段> --like <模板正则> --mesh <道具.glb> --preset spin --at x,y,z --rpm <值> --axis vertical
wowsmod model anim-sample <序列.seq>
```
`--preset` 为 spin/orbit/float；其余参数读 `wowsmod model animate --help`。产出 misc 下 geometry/visual/model/seq/ssm 与材质，并尝试把 ssm 挂到分段 model 的 animations。`anim-sample` 证明序列采样，工作台播放证明预览实现，两者都不证明游戏加载或运动。

**范围**：工作台会循环采样(最多 240 帧，长序列只取前 60 秒)。历史实机存在 misc 网格未加载、物件消失的问题；欧拉角单位/顺序与当前客户端挂载格式仍需证据。缺少当前 dump 与实机记录时，26.11 动画标未知，不能宣布新增支持。

**验收**：先用静态 `model import` 确认网格、挂点、材质、LOD 与可见性；再在独立小样验证动画。按授权安装与进港，分开检查文件、注册日志，以及相隔数秒的同机位画面/视频(方向、速度、位置)；任一环失败写具体限制，可保留已验证的静态方案。旧案例见 `workflow.changelog`。
