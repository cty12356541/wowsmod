#!/usr/bin/env bash
# 下载多材质测试模型到本地(不入库)。用法: fetch_test_models.sh <目录>
# AlphaBlendModeTest: © 2018 Analytical Graphics, Inc., CC-BY-4.0(作者 Ed Mackey), Khronos glTF-Sample-Assets。
#   6 个材质, 其中 5 个共用一张 PNG 图集(OPAQUE / MASK 0.25 / 0.75 / 默认 / BLEND, 双面), 底座为 JPEG 贴图。
#   用于验证: 多材质拆分、共用图集只存一份、doubleSided、alphaTest、JPEG 解码。
set -euo pipefail
d="${1:-test_models}"; mkdir -p "$d"; cd "$d"
base=https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/main/Models/AlphaBlendModeTest
curl -fsSL -o AlphaBlendModeTest.glb "$base/glTF-Binary/AlphaBlendModeTest.glb"
curl -fsSL -o AlphaBlendModeTest.LICENSE.md "$base/LICENSE.md"
curl -fsSL -o AlphaBlendModeTest.README.md "$base/README.md"
sha256sum AlphaBlendModeTest.glb
