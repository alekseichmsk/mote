# Soak-test local Mote llama-server for ~30 minutes with varied RU inputs.
$ErrorActionPreference = "Stop"
$bin = Join-Path $env:APPDATA "Mote\bin\llama-server.exe"
$model = Join-Path $env:APPDATA "Mote\models\Qwen2.5-3B-Instruct-Q4_K_M.gguf"
$work = Join-Path $env:APPDATA "Mote\bin"
$outDir = Join-Path $PSScriptRoot "llm-soak-out"
$casesPath = Join-Path $PSScriptRoot "llm-soak-cases.json"
$port = 18766
$durationSec = 30 * 60
$system = "Fix Russian spelling, punctuation, and obvious typos. Keep meaning and language. Return ONLY the corrected text, no quotes, no explanations."

if (-not (Test-Path $bin)) { throw "missing runtime" }
if (-not (Test-Path $model)) { throw "missing model" }
if (-not (Test-Path $casesPath)) { throw "missing cases json" }
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$logPath = Join-Path $outDir ("soak-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".jsonl")
$summaryPath = Join-Path $outDir ("summary-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".md")
$cases = Get-Content -Path $casesPath -Encoding UTF8 -Raw | ConvertFrom-Json

$p = Start-Process -FilePath $bin `
  -ArgumentList @("--host", "127.0.0.1", "--port", "$port", "-m", $model, "-c", "2048") `
  -WorkingDirectory $work `
  -WindowStyle Hidden `
  -PassThru

function Wait-Healthy {
  for ($i = 0; $i -lt 120; $i++) {
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

function Invoke-Improve([string]$text) {
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

try {
  if (-not (Wait-Healthy)) { throw "sidecar not healthy" }
  Write-Host "sidecar healthy pid=$($p.Id) port=$port"
  Write-Host "log=$logPath"
  Write-Host "duration=${durationSec}s cases=$($cases.Count)"

  $started = Get-Date
  $deadline = $started.AddSeconds($durationSec)
  $round = 0
  $total = 0
  $okHttp = 0
  $failHttp = 0
  $expectHits = 0
  $expectMiss = 0
  $expectN = 0
  $lat = New-Object System.Collections.Generic.List[int]
  $byCase = @{}
  foreach ($c in $cases) {
    $byCase[$c.id] = @{ n = 0; httpOk = 0; expectOk = 0; expectN = 0; lastOut = ""; lastMs = 0 }
  }

  while ((Get-Date) -lt $deadline) {
    $round++
    $mins = [math]::Round(((Get-Date) - $started).TotalMinutes, 1)
    Write-Host "--- round $round elapsed=${mins}m ---"
    foreach ($c in $cases) {
      if ((Get-Date) -ge $deadline) { break }
      $total++
      $row = [ordered]@{
        ts = (Get-Date).ToString("o")
        round = $round
        id = $c.id
        input = $c.in
      }
      try {
        $res = Invoke-Improve $c.in
        $okHttp++
        $lat.Add([int]$res.ms) | Out-Null
        $byCase[$c.id].n++
        $byCase[$c.id].httpOk++
        $byCase[$c.id].lastOut = $res.output
        $byCase[$c.id].lastMs = $res.ms
        $row.ok = $true
        $row.ms = $res.ms
        $row.output = $res.output
        $row.changed = ($res.output -ne $c.in)
        $expects = @($c.expectContains)
        if ($expects.Count -gt 0) {
          $expectN++
          $byCase[$c.id].expectN++
          $hit = $true
          foreach ($frag in $expects) {
            if ([string]::IsNullOrEmpty($frag)) { continue }
            if ($res.output.ToLowerInvariant().IndexOf($frag.ToLowerInvariant()) -lt 0) {
              $hit = $false
              break
            }
          }
          if ($hit) { $expectHits++; $byCase[$c.id].expectOk++ } else { $expectMiss++ }
          $row.expectHit = $hit
        } else {
          $row.expectHit = $null
        }
        Write-Host ("{0} {1}ms changed={2} expect={3}" -f $c.id, $res.ms, $row.changed, $row.expectHit)
      } catch {
        $failHttp++
        $byCase[$c.id].n++
        $row.ok = $false
        $row.error = "$_"
        Write-Host ("{0} FAIL {1}" -f $c.id, $_)
      }
      ($row | ConvertTo-Json -Compress) | Add-Content -Path $logPath -Encoding UTF8
    }
  }

  $elapsed = ((Get-Date) - $started).TotalMinutes
  $p50 = $null; $p95 = $null; $avg = $null
  if ($lat.Count -gt 0) {
    $sorted = $lat.ToArray() | Sort-Object
    $avg = [math]::Round(($sorted | Measure-Object -Average).Average, 0)
    $p50 = $sorted[[math]::Floor(($sorted.Count - 1) * 0.50)]
    $p95 = $sorted[[math]::Floor(($sorted.Count - 1) * 0.95)]
  }
  $expectRate = if ($expectN -gt 0) { [math]::Round(100.0 * $expectHits / $expectN, 1) } else { 0 }

  $md = New-Object System.Collections.Generic.List[string]
  $md.Add("# LLM soak report") | Out-Null
  $md.Add("") | Out-Null
  $md.Add("- Started: $($started.ToString('o'))") | Out-Null
  $md.Add("- Elapsed: $([math]::Round($elapsed, 1)) min (target 30)") | Out-Null
  $md.Add("- Sidecar: 127.0.0.1:$port") | Out-Null
  $md.Add("- Model: Qwen2.5-3B-Instruct-Q4_K_M.gguf") | Out-Null
  $md.Add("- Rounds: $round") | Out-Null
  $md.Add("- Requests: $total (http ok=$okHttp fail=$failHttp)") | Out-Null
  $md.Add("- Latency ms: avg=$avg p50=$p50 p95=$p95") | Out-Null
  $md.Add("- Expect-contains hit rate: $expectHits/$expectN ($expectRate%)") | Out-Null
  $md.Add("- Log: ``$logPath``") | Out-Null
  $md.Add("") | Out-Null
  $md.Add("## Per case (last output)") | Out-Null
  $md.Add("") | Out-Null
  foreach ($c in $cases) {
    $s = $byCase[$c.id]
    $rate = if ($s.expectN -gt 0) { "{0}/{1}" -f $s.expectOk, $s.expectN } else { "n/a" }
    $md.Add("### $($c.id)") | Out-Null
    $md.Add("- input: $($c.in)") | Out-Null
    $md.Add("- runs: $($s.n), http ok: $($s.httpOk), expect: $rate, last ms: $($s.lastMs)") | Out-Null
    $md.Add("- last out: $($s.lastOut)") | Out-Null
    $md.Add("") | Out-Null
  }
  [System.IO.File]::WriteAllLines($summaryPath, $md, (New-Object System.Text.UTF8Encoding $false))
  Write-Host "SUMMARY=$summaryPath"
  Write-Host ("DONE total={0} ok={1} fail={2} expect={3}% avgMs={4}" -f $total, $okHttp, $failHttp, $expectRate, $avg)
} finally {
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
