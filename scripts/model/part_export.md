# 官方部件 → Blender 比例尺
```
wowsmod model export-blender --project <p> -o /tmp/ship.glb
# Blender: 只保留需要作比例的 hull/bridge 作参考层(不可编辑渲染), 新建道具物体
# 导出道具单独 glb → model import
```
禁整船重导替换(易丢 LOD); 优先补丁式外带+罩壳。
