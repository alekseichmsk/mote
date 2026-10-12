# Bench Mote llama-server on CPU then CUDA GPU: latency + RAM.
$ErrorActionPreference = "Stop"
$root = Join-Path $env:APPDATA "Mote"
$binRoot = Join-Path $root "bin"
$cpuDir = Join-Path $binRoot "cpu"
$cudaDir = Join-Path $binRoot "cuda"
$model = Join-Path $root "models\Qwen2.5-3B-Instruct-Q4_K_M.gguf"
$outDir = Join-Path $PSScriptRoot "llm-soak-out"
$casesPath = Join-Path $PSScriptRoot "llm-soak-cases.json"
$tag = "b9992"
$cpuZipUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$tag/llama-$tag-bin-win-cpu-x64.zip"
$cudaZipUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$tag/llama-$tag-bin-win-cuda-12.4-x64.zip"
$cudartZipUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$tag/cudart-llama-bin-win-cuda-12.4-x64.zip"
$system = "Fix Russian spelling, punctuation, and obvious typos. Keep meaning and language. Return ONLY the corrected text, no quotes, no explanations."
$rounds = 3
$portCpu = 18771
$portGpu = 18772

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$reportPath = Join-Path $outDir ("device-bench-$stamp.md")
$jsonPath = Join-Path $outDir ("device-bench-$stamp.json")

function Ensure-Dir($p) { New-Item -ItemType Directory -Force -Path $p | Out-Null }

function Migrate-LegacyCpu {
  $legacy = Join-Path $binRoot "llama-server.exe"
  $cpuExe = Join-Path $cpuDir "llama-server.exe"
  if ((Test-Path $legacy) -and -not (Test-Path $cpuExe)) {
    Write-Host "[bench] migrate flat bin -> bin\cpu"
    Ensure-Dir $cpuDir
    Get-ChildItem $binRoot -File | ForEach-Object {
      Move-Item -Force $_.FullName (Join-Path $cpuDir $_.Name)
    }
  }
}

function Test-RuntimeReady([string]$dir, [string]$kind) {
  $exe = Join-Path $dir "llama-server.exe"
  if (-not (Test-Path $exe)) { return $false }
  $impl = Join-Path $dir "llama-server-impl.dll"
  if ((Test-Path $impl) -and ((Get-Item $impl).Length -lt 100000)) { return $false }
  if ($kind -eq "gpu") {
    $files = Get-ChildItem $dir -File -Recurse -ErrorAction SilentlyContinue | ForEach-Object { $_.Name.ToLowerInvariant() }
    $backend = $files | Where-Object { $_ -like "*ggml-cuda*" }
    $runtime = $files | Where-Object { $_ -like "cudart*" -or $_ -like "cublas*" }
    return ($null -ne $backend -and $null -ne $runtime)
  }
  return $true
}

function Flatten-Runtime([string]$dir) {
  Get-ChildItem $dir -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
    $_.Extension -match '\.(dll|exe)$'
  } | ForEach-Object {
    $dst = Join-Path $dir $_.Name
    if ($_.FullName -ne $dst) {
      Copy-Item -Force $_.FullName $dst
    }
  }
}

function Download-AndExtract([string]$url, [string]$dir, [string]$label) {
  Ensure-Dir $dir
  $zip = Join-Path $dir ($label + ".zip")
  Write-Host "[bench] download $label ..."
  Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
  Write-Host "[bench] extract $label -> $dir"
  & tar -xf $zip -C $dir
  Remove-Item -Force $zip -ErrorAction SilentlyContinue
  Flatten-Runtime $dir
}

function Ensure-CpuRuntime {
  Ensure-Dir $cpuDir
  if (Test-RuntimeReady $cpuDir "cpu") { return }
  if (Test-Path (Join-Path $binRoot "llama-server.exe")) {
    Migrate-LegacyCpu
  }
  if (-not (Test-RuntimeReady $cpuDir "cpu")) {
    Download-AndExtract $cpuZipUrl $cpuDir "llama-cpu-$tag"
  }
  if (-not (Test-RuntimeReady $cpuDir "cpu")) { throw "CPU runtime not ready" }
}

function Ensure-CudaRuntime {
  Ensure-Dir $cudaDir
  if (Test-RuntimeReady $cudaDir "gpu") { return }
  Download-AndExtract $cudaZipUrl $cudaDir "llama-cuda-$tag"
  Download-AndExtract $cudartZipUrl $cudaDir "cudart-$tag"
  if (-not (Test-RuntimeReady $cudaDir "gpu")) { throw "CUDA runtime not ready (need ggml-cuda + cudart)" }
}

function Stop-LlamaOnPort([int]$port) {
  Get-NetTCPConnection -LocalPort $port -ErrorAction SilentlyContinue |
    ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
  Get-Process llama-server -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 1
}

function Wait-Healthy([int]$port) {
  for ($i = 0; $i -lt 180; $i++) {
    try {
      $h = Invoke-WebRequest -Uri "http://127.0.0.1:$port/health" -UseBasicParsing -TimeoutSec 2
      if ($h.StatusCode -eq 200) { return $true }
    } catch {}
    try {
      $h = Invoke-WebRequest -Uri "http://127.0.0.1:$port/v1/models" -UseBasicParsing -TimeoutSec 2
      if ($h.StatusCode -eq 200) { return $true }
    } catch {}
    Start-Sleep -Milliseconds 500
  }
  return $false
}

function Sample-Ram([int]$procId) {
  try {
    $p = Get-Process -Id $procId -ErrorAction Stop
    return [math]::Round($p.WorkingSet64 / 1MB, 1)
  } catch { return 0 }
}

function Sample-MoteRam {
  $sum = 0.0
  Get-Process mote, msedgewebview2 -ErrorAction SilentlyContinue | ForEach-Object {
    if ($_.ProcessName -eq "mote" -or $_.MainWindowTitle -or $_.CommandLine -match "mote") {
      # Best-effort: all mote.exe + webview with mote in path is hard; sum mote.exe only + all webview tied later
    }
  }
  $mote = Get-Process mote -ErrorAction SilentlyContinue | Measure-Object WorkingSet64 -Sum
  if ($mote) { $sum += $mote.Sum }
  # WebView processes for mote are under msedgewebview2; approximate via user-data-dir is heavy — skip exact, report mote.exe
  return [math]::Round($sum / 1MB, 1)
}

function Get-NvidiaGpuMemMb {
  try {
    $line = & nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits 2>$null
    if ($line) { return [int]($line | Select-Object -First 1) }
  } catch {}
  return $null
}

function Invoke-Improve([int]$port, [string]$text) {
  $payload = @{
    model = "local"
    temperature = 0
    max_tokens = 256
    messages = @(
      @{ role = "system"; content = $system }
      @{ role = "user"; content = $text }
    )
  }
  $json = $payload | ConvertTo-Json -Depth 8 -Compress
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($json)
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $r = Invoke-RestMethod -Uri "http://127.0.0.1:$port/v1/chat/completions" `
    -Method Post -ContentType "application/json; charset=utf-8" -Body $bytes -TimeoutSec 180
  $sw.Stop()
  return @{
    output = ([string]$r.choices[0].message.content).Trim()
    ms = [int]$sw.ElapsedMilliseconds
  }
}

function Run-DeviceBench([string]$device, [string]$workDir, [int]$port, [int]$ngl, $cases) {
  Write-Host "`n======== BENCH $device ngl=$ngl ========"
  Stop-LlamaOnPort $port
  $exe = Join-Path $workDir "llama-server.exe"
  if (-not (Test-Path $exe)) { throw "missing $exe" }
  if (-not (Test-Path $model)) { throw "missing model" }

  $gpuBefore = Get-NvidiaGpuMemMb
  $bootSw = [System.Diagnostics.Stopwatch]::StartNew()
  $p = Start-Process -FilePath $exe `
    -ArgumentList @("--host", "127.0.0.1", "--port", "$port", "-m", $model, "-c", "2048", "-ngl", "$ngl") `
    -WorkingDirectory $workDir `
    -WindowStyle Hidden `
    -PassThru
  if (-not (Wait-Healthy $port)) {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    throw "sidecar not healthy ($device)"
  }
  $bootSw.Stop()
  $ramIdle = Sample-Ram $p.Id
  $gpuIdle = Get-NvidiaGpuMemMb
  $moteRam = Sample-MoteRam
  Write-Host "[bench] $device healthy pid=$($p.Id) bootMs=$($bootSw.ElapsedMilliseconds) ramIdleMB=$ramIdle moteMB=$moteRam gpuMemMB=$gpuIdle"

  $lat = New-Object System.Collections.Generic.List[int]
  $ramPeak = $ramIdle
  $ok = 0
  $fail = 0
  $rows = @()
  $wall = [System.Diagnostics.Stopwatch]::StartNew()
  for ($r = 1; $r -le $rounds; $r++) {
    foreach ($c in $cases) {
      try {
        $res = Invoke-Improve $port ([string]$c.input)
        $lat.Add([int]$res.ms) | Out-Null
        $ok++
        $ramNow = Sample-Ram $p.Id
        if ($ramNow -gt $ramPeak) { $ramPeak = $ramNow }
        $rows += [pscustomobject]@{
          device = $device; round = $r; id = $c.id; ms = $res.ms; ok = $true; output = $res.output
        }
        Write-Host ("[bench] {0} r{1} {2} {3}ms" -f $device, $r, $c.id, $res.ms)
      } catch {
        $fail++
        $rows += [pscustomobject]@{
          device = $device; round = $r; id = $c.id; ms = -1; ok = $false; output = "$_"
        }
        Write-Host ("[bench] {0} r{1} {2} FAIL {3}" -f $device, $r, $c.id, $_)
      }
    }
  }
  $wall.Stop()
  $gpuPeak = Get-NvidiaGpuMemMb
  $ramEnd = Sample-Ram $p.Id

  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  try { Wait-Process -Id $p.Id -TimeoutSec 10 } catch {}
  Start-Sleep -Seconds 2
  $gpuAfter = Get-NvidiaGpuMemMb

  $avg = if ($lat.Count) { [int](($lat | Measure-Object -Average).Average) } else { 0 }
  $p50 = if ($lat.Count) { ($lat | Sort-Object)[[int](($lat.Count - 1) * 0.5)] } else { 0 }
  $p95 = if ($lat.Count) { ($lat | Sort-Object)[[int](($lat.Count - 1) * 0.95)] } else { 0 }
  $min = if ($lat.Count) { ($lat | Measure-Object -Minimum).Minimum } else { 0 }
  $max = if ($lat.Count) { ($lat | Measure-Object -Maximum).Maximum } else { 0 }

  return [ordered]@{
    device = $device
    ngl = $ngl
    port = $port
    bootMs = [int]$bootSw.ElapsedMilliseconds
    wallMs = [int]$wall.ElapsedMilliseconds
    ok = $ok
    fail = $fail
    avgMs = $avg
    p50Ms = [int]$p50
    p95Ms = [int]$p95
    minMs = [int]$min
    maxMs = [int]$max
    llamaRamIdleMB = $ramIdle
    llamaRamPeakMB = $ramPeak
    llamaRamEndMB = $ramEnd
    moteRamMB = $moteRam
    gpuMemBeforeMB = $gpuBefore
    gpuMemIdleMB = $gpuIdle
    gpuMemPeakMB = $gpuPeak
    gpuMemAfterMB = $gpuAfter
    rows = $rows
  }
}

if (-not (Test-Path $model)) { throw "missing model: $model" }
if (-not (Test-Path $casesPath)) { throw "missing cases" }
$cases = Get-Content -Path $casesPath -Encoding UTF8 -Raw | ConvertFrom-Json

Write-Host "[bench] ensure CPU runtime..."
Ensure-CpuRuntime
Write-Host "[bench] ensure CUDA runtime (may download ~600MB)..."
Ensure-CudaRuntime

$cpu = Run-DeviceBench "cpu" $cpuDir $portCpu 0 $cases
Start-Sleep -Seconds 3
$gpu = Run-DeviceBench "gpu" $cudaDir $portGpu 99 $cases

$speedup = if ($gpu.avgMs -gt 0) { [math]::Round($cpu.avgMs / $gpu.avgMs, 2) } else { 0 }
$ramDelta = [math]::Round($gpu.llamaRamPeakMB - $cpu.llamaRamPeakMB, 1)
$vramDelta = $null
if ($null -ne $gpu.gpuMemPeakMB -and $null -ne $gpu.gpuMemBeforeMB) {
  $vramDelta = $gpu.gpuMemPeakMB - $gpu.gpuMemBeforeMB
}

$report = @"
# Device bench CPU vs GPU

stamp: $stamp
model: Qwen2.5-3B-Instruct-Q4_K_M.gguf
rounds: $rounds × $($cases.Count) cases = $($cpu.ok + $cpu.fail) req/device
ngl: CPU=0, GPU=99

## CPU
- boot: $($cpu.bootMs) ms
- wall (all requests): $($cpu.wallMs) ms
- latency avg/p50/p95/min/max: $($cpu.avgMs) / $($cpu.p50Ms) / $($cpu.p95Ms) / $($cpu.minMs) / $($cpu.maxMs) ms
- ok/fail: $($cpu.ok)/$($cpu.fail)
- llama-server RAM idle/peak/end: $($cpu.llamaRamIdleMB) / $($cpu.llamaRamPeakMB) / $($cpu.llamaRamEndMB) MB
- mote.exe RAM (approx): $($cpu.moteRamMB) MB
- nvidia-smi used (idle/peak): $($cpu.gpuMemIdleMB) / $($cpu.gpuMemPeakMB) MB

## GPU (CUDA)
- boot: $($gpu.bootMs) ms
- wall (all requests): $($gpu.wallMs) ms
- latency avg/p50/p95/min/max: $($gpu.avgMs) / $($gpu.p50Ms) / $($gpu.p95Ms) / $($gpu.minMs) / $($gpu.maxMs) ms
- ok/fail: $($gpu.ok)/$($gpu.fail)
- llama-server RAM idle/peak/end: $($gpu.llamaRamIdleMB) / $($gpu.llamaRamPeakMB) / $($gpu.llamaRamEndMB) MB
- mote.exe RAM (approx): $($gpu.moteRamMB) MB
- nvidia-smi used before/idle/peak/after: $($gpu.gpuMemBeforeMB) / $($gpu.gpuMemIdleMB) / $($gpu.gpuMemPeakMB) / $($gpu.gpuMemAfterMB) MB

## Diff (CPU → GPU)
- avg latency: $($cpu.avgMs) → $($gpu.avgMs) ms (speedup ×$speedup)
- wall: $($cpu.wallMs) → $($gpu.wallMs) ms
- llama RAM peak: $($cpu.llamaRamPeakMB) → $($gpu.llamaRamPeakMB) MB (delta $ramDelta MB)
- VRAM delta (peak − before): $vramDelta MB
"@

$report | Set-Content -Path $reportPath -Encoding UTF8
$payload = [ordered]@{
  stamp = $stamp
  rounds = $rounds
  cpu = $cpu
  gpu = $gpu
  speedupAvg = $speedup
  ramPeakDeltaMB = $ramDelta
  vramDeltaMB = $vramDelta
}
# Drop nested rows duplication for size: keep rows in json
($payload | ConvertTo-Json -Depth 8) | Set-Content -Path $jsonPath -Encoding UTF8

Write-Host "`n$report"
Write-Host "`n[bench] report=$reportPath"
Write-Host "[bench] json=$jsonPath"
