<#
验收 M1-5: geometry 往返逐字节一致。
  1) cargo test(含 18 样本集成测试, 要求样本存在);
  2) wowsmod geom testdata\geometry18 --roundtrip;
  3) (可选 -All) 游戏内全部 *.geometry 往返。
#>
param([switch]$All, [string]$Game, [string]$Exe)
. "$PSScriptRoot\_common.ps1"
$wm = Get-WowsmodExe $Exe
$gargs = @(); if ($Game) { $gargs = @('--game', $Game) }
$base = New-CleanDir (Join-Path $Root 'out\accept_roundtrip')
if (-not (Test-Path (Join-Path $Root 'testdata\geometry18'))) { & "$PSScriptRoot\fetch_samples.ps1" -Game $Game -Exe $Exe }
Push-Location $Root
$env:WOWSMOD_REQUIRE_SAMPLES = '1'
# PowerShell 5 会把 cargo 写到 stderr 的 "Compiling..." 当成 NativeCommandError; 用 cmd 重定向只看退出码
$cargoLog = Join-Path $base 'cargo_test.txt'
cmd /c "cargo test --release > `"$cargoLog`" 2>&1"
$testCode = $LASTEXITCODE
Pop-Location
Select-String -Path $cargoLog -Pattern 'test result|panicked|FAILED' | ForEach-Object { $_.Line } | Write-Host
$r = Invoke-WowsmodJson $wm @('geom', (Join-Path $Root 'testdata\geometry18'), '--roundtrip')
$r.Raw | Set-Content -Encoding UTF8 (Join-Path $base 'geom18.json')
$s = $r.Json.summary
Write-Host "18 样本: 文件 $($s.files) / 重排版一致 $($s.relayout_identical) / 解码再编码一致 $($s.reencode_identical) / 失败 $($s.errors)"
$allOk = $true
if ($All) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $ra = Invoke-WowsmodJson $wm ($gargs + @('geom', '*.geometry', '--roundtrip'))
    $ra.Raw | Set-Content -Encoding UTF8 (Join-Path $base 'geom_all.json')
    $sa = $ra.Json.summary
    Write-Host "全部游戏 geometry: 文件 $($sa.files) / 重排版一致 $($sa.relayout_identical) / 解码再编码一致 $($sa.reencode_identical) / 失败 $($sa.errors) / 用时 $([Math]::Round($sw.Elapsed.TotalSeconds,1))s"
    $allOk = ($sa.errors -eq 0) -and ($sa.reencode_identical -eq $sa.files)
}
if ($testCode -eq 0 -and $s.reencode_identical -eq 18 -and $s.relayout_identical -eq 18 -and $allOk) { Write-Host '通过' -ForegroundColor Green; exit 0 }
Write-Host '未通过' -ForegroundColor Red; exit 1
