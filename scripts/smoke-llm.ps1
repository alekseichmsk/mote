$ErrorActionPreference = "Stop"
$bin = Join-Path $env:APPDATA "Mote\bin\llama-server.exe"
$model = Join-Path $env:APPDATA "Mote\models\Qwen2.5-3B-Instruct-Q4_K_M.gguf"
$work = Join-Path $env:APPDATA "Mote\bin"
$port = 18765
$bodyPath = Join-Path $env:TEMP "mote-smoke-llm.json"

if (-not (Test-Path $bin)) { throw "missing runtime: $bin" }
if (-not (Test-Path $model)) { throw "missing model: $model" }

# UTF-8 JSON (Cyrillic) written without PS string encoding issues.
$utf8 = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText(
  $bodyPath,
  '{"model":"local","temperature":0,"max_tokens":64,"messages":[{"role":"system","content":"Fix spelling. Return only corrected text."},{"role":"user","content":"\u043f\u0440\u0435\u0432\u0435\u0442"}]}',
  $utf8
)

$p = Start-Process -FilePath $bin `
  -ArgumentList @("--host", "127.0.0.1", "--port", "$port", "-m", $model, "-c", "2048") `
  -WorkingDirectory $work `
  -WindowStyle Hidden `
  -PassThru

try {
  $ok = $false
  for ($i = 0; $i -lt 90; $i++) {
    try {
      $h = Invoke-WebRequest -Uri "http://127.0.0.1:$port/health" -UseBasicParsing -TimeoutSec 2
      if ($h.StatusCode -eq 200) { $ok = $true; break }
    } catch {}
    try {
      $h = Invoke-WebRequest -Uri "http://127.0.0.1:$port/v1/models" -UseBasicParsing -TimeoutSec 2
      if ($h.StatusCode -eq 200) { $ok = $true; break }
    } catch {}
    Start-Sleep -Milliseconds 500
  }
  Write-Host "healthy=$ok pid=$($p.Id)"
  if (-not $ok) { throw "sidecar not healthy" }

  $bytes = [System.IO.File]::ReadAllBytes($bodyPath)
  $r = Invoke-RestMethod -Uri "http://127.0.0.1:$port/v1/chat/completions" `
    -Method Post `
    -ContentType "application/json; charset=utf-8" `
    -Body $bytes `
    -TimeoutSec 120

  $out = $r.choices[0].message.content
  Write-Host "improved=$out"
  if ([string]::IsNullOrWhiteSpace($out)) { throw "empty improve" }
  Write-Host "LLM_SMOKE=OK"
} finally {
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Remove-Item $bodyPath -ErrorAction SilentlyContinue
}
