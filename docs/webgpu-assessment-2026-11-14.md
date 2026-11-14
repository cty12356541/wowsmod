# WebGPU 渲染后端评估

2026-11-14，工作台基线 `f094f31`，Three.js r147。结论：本轮保留当前 WebGL 默认后端。WebGPU 能力探测通过，但尚无这个 Mod 的 WebGPU 画质或性能对照，不能把切换 API 当作画质升级。

## 本机证据

在独立本地测试页、全新无头 Edge 配置中探测，没有操作用户浏览器或加载用户项目。Edge `147.0.3912.60` 暴露 `navigator.gpu`；高性能 adapter 成功返回 NVIDIA Blackwell，`isFallbackAdapter=false`，`requestDevice()` 成功。系统 GPU 信息为 RTX 5060 Ti，驱动 `32.0.15.9621`。可用特性包括 BC 纹理压缩、f16、timestamp-query；最大二维纹理尺寸为 16384。单次 adapter/device 初始化约 908 / 113 ms，仅是能力探测时序，不能当作模型加载或渲染性能。

原始结果与复现脚本：`out/workbench-completion-20261114/webgpu/capability.json`、`probe.py`。测试服务和自有 Edge 已关闭。探测对象是外部 Edge；Codex 内部浏览器与其他 GPU/驱动的能力没有据此推定。

## 当前源码的迁移边界

- `preview_app/web/src/engine/viewer.ts` 使用 `WebGLRenderer`。舰体材质通过 `onBeforeCompile` 注入通道解释、发光等逻辑。官方文档限定该钩子用于 WebGL。[Material 文档](https://threejs.org/docs/pages/Material.html)
- 天空、海面、切面、接触阴影、水线、泡沫和合成使用 `ShaderMaterial`；`engine/sim.ts` 的粒子也使用自定义 GLSL。这个材质类只支持 WebGL。[ShaderMaterial 文档](https://threejs.org/docs/pages/ShaderMaterial.html)
- 后处理实际是 `UnrealBloomPass`、两个 `WebGLRenderTarget` 和自定义合成链，没有实例化 `EffectComposer`。渲染器切换还涉及目标纹理、颜色空间、裁剪、透明顺序、GPU 读回和出图流程。
- 当前 glTF 骨骼绑定、真实炮口和 CPU 运动逻辑可以作为迁移输入，但必须验证新材质下蒙皮、法线和动态效果，不以白模能显示视为通过。

当前 Three.js 官方迁移指南要求自定义 shader 与 `onBeforeCompile` 迁移到 node materials / TSL，后处理使用新节点系统。WebGPURenderer 可以回退至 WebGL 2，这不等于旧自定义材质自动兼容。官方也提示不同场景仍可能由 WebGL 获得更好的性能。[WebGPURenderer 指南](https://threejs.org/manual/pages/webgpurenderer.html)

## 画质和性能判断

更接近游戏的画面取决于贴图通道含义、BRDF、光照环境、曝光与色调映射、透明和特效参数。本项目目前没有游戏渲染器的完整参考参数；更换底层 API 不会自动补齐它们。保真升级应先建立固定游戏截图、共同相机与光照、明确材质通道，再分别校准结果。

现有真实船 WebGL 测试已达到本机约 165 Hz 帧回调上限。航行同步读回帧中位 2.10 ms；这一数字包含读回成本，不能当作纯 GPU 时间。当前加载瓶颈是贴图请求和不可见基准方案的加载，WebGPU 无法替代这项资源调度修复。证据见 `out/closure-20261114/perf/report.md`；这份测量只评价当前 WebGL，未与 WebGPU 比速。

## 决策及未来迁移条件

本轮交付为源码兼容性评估与本机能力探测，未移植 shader、未构建同材质 WebGPU 原型，也未做跨后端图像或帧时间对照。因此不设置一个会丢材质/效果的实验开关，不升级默认 Three.js 依赖。

若以后选择迁移，建议在独立依赖和入口中完成四个验收阶段：同一真实 GLB 与原贴图的 PBS/normal/MG/AO/发光、透明；14 个真实蒙皮炮位与 37 个炮口；海面/尾迹/开火/bloom 与色调映射；固定数值相机、实际像素尺寸、DPR 和画质条件下至少三次冷暖加载与帧时间对照。只有图像和功能保真、设备兼容且存在实测收益，才改变默认后端。以上是明确的后续迁移条件，不是本轮未完成的实现任务。
