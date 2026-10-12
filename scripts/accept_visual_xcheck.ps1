<#
交叉验证: SDK 文本 .visual(renderSet 节点名 → murmur3 mapping id)与 assets.bin 二进制 VisualPrototype 两种配对来源
得到的 (顶点 mapping, 索引 mapping) 是否完全相同。两者来源独立, 一致即说明 assets.bin 解析正确。
材质名在 XML 里是模板/类型名(如 tl2_shipmat_pbs_director), 在 assets.bin 里是 .mfm 路径, 本就不相同; 报告里单独列出供参考。
SDK 目录默认取 M0 实验中游戏 extractSources 生成的勃艮第 SDK 子集(只读)。
结果: out\accept_visual_xcheck\report.json
#>
param([string]$SdkDir = '<WORK_DIR>\m0\ingame\mods\exp2_sdk\res_mods\PnFMods\ModsSDK.zip\FSB510_Bourgogne', [string]$Game, [string]$Exe)
. "$PSScriptRoot\_common.ps1"
$wm = Get-WowsmodExe $Exe
$gargs = @(); if ($Game) { $gargs = @('--game', $Game) }
$base = New-CleanDir (Join-Path $Root 'out\accept_visual_xcheck')
$visuals = [IO.Directory]::GetFiles($SdkDir, '*.visual', [IO.SearchOption]::AllDirectories) | Sort-Object
Write-Host "SDK visual: $($visuals.Count) 个 ($SdkDir)"
$geomCache = @{}
$rows = foreach ($v in $visuals) {
    $stem = [IO.Path]::GetFileNameWithoutExtension($v)
    $lod = '0'; $g = $stem
    $m = [regex]::Match($stem, '^(.*)_lod(\d+)$')
    if ($m.Success) { $g = $m.Groups[1].Value; $lod = $m.Groups[2].Value }
    if (-not $geomCache.ContainsKey($g)) {
        $f = Invoke-WowsmodJson $wm ($gargs + @('find', "$g.geometry"))
        $geomCache[$g] = @($f.Json.files | ForEach-Object { $_.path })
    }
    $gp = $geomCache[$g]
    if ($gp.Count -ne 1) { [pscustomobject]@{ visual = $stem; geometry = $g; lod = $lod; status = "跳过: 游戏中同名 geometry 有 $($gp.Count) 个"; xml_pairs = 0; assets_pairs = 0; material_names_differ = $false }; continue }
    $x = Invoke-WowsmodJson $wm ($gargs + @('export', $gp[0], '-o', (Join-Path $base "$stem.xml.glb"), '--pairing', 'visual', '--visual', $v))
    $a = Invoke-WowsmodJson $wm ($gargs + @('export', $gp[0], '-o', (Join-Path $base "$stem.ab.glb"), '--pairing', 'assets', '--lod', $lod))
    if (-not $x.Json.results) { [pscustomobject]@{ visual = $stem; geometry = $g; lod = $lod; status = "XML 配对不可用: $(($x.Json.error) -join ' ')"; xml_pairs = 0; assets_pairs = @($a.Json.results[0].pairs).Count; material_names_differ = $false }; continue }
    if (-not $a.Json.results) { [pscustomobject]@{ visual = $stem; geometry = $g; lod = $lod; status = "assets.bin 配对失败: $(($a.Json.error) -join ' ')"; xml_pairs = @($x.Json.results[0].pairs).Count; assets_pairs = 0; material_names_differ = $false }; continue }
    $mapKey = { param($p) "$($p.vertices_mapping)/$($p.indices_mapping)" }
    $matOf = { param($p) ((($p.material -replace '\\','/') -split '/')[-1]).ToLower() }
    $xs = @($x.Json.results[0].pairs | ForEach-Object { & $mapKey $_ }) | Sort-Object
    $as = @($a.Json.results[0].pairs | ForEach-Object { & $mapKey $_ }) | Sort-Object
    $same = (($xs -join ';') -eq ($as -join ';'))
    $xmlMats = @($x.Json.results[0].pairs | ForEach-Object { & $matOf $_ }) | Sort-Object -Unique
    $abMats = @($a.Json.results[0].pairs | ForEach-Object { & $matOf $_ }) | Sort-Object -Unique
    $matDiffer = (($xmlMats -join ';') -ne ($abMats -join ';'))
    [pscustomobject]@{
        visual = $stem; geometry = $g; lod = $lod
        status = $(if ($same) { '一致' } else { '不一致' })
        xml_pairs = $xs.Count; assets_pairs = $as.Count
        assets_visual = $a.Json.results[0].pairing
        material_names_differ = $matDiffer
        xml_materials = ($xmlMats -join ',')
        assets_materials = ($abMats -join ',')
        diff = $(if ($same) { '' } else { "XML-only: $(($xs | Where-Object { $as -notcontains $_ }) -join ',') ; AB-only: $(($as | Where-Object { $xs -notcontains $_ }) -join ',')" })
    }
}
$rows | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 (Join-Path $base 'report.json')
$rows | Format-Table visual, lod, xml_pairs, assets_pairs, status, material_names_differ -AutoSize | Out-String -Width 200 | Write-Host
$cmp = @($rows | Where-Object { $_.status -in '一致', '不一致' })
$okN = @($cmp | Where-Object status -eq '一致').Count
$matDiff = @($cmp | Where-Object material_names_differ).Count
Write-Host "可比较 $($cmp.Count) 个 visual, mapping 一致 $okN, 材质名不同(预期) $matDiff, 渲染集合计 $(($cmp | Measure-Object xml_pairs -Sum).Sum); 其余: $(@($rows).Count - $cmp.Count) 个跳过/不可用"
if ($cmp.Count -gt 0 -and $okN -eq $cmp.Count) { exit 0 } else { exit 1 }
