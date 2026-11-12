# Spike S3: the current setup — ElectroChat's vLLM (WSL distro LocalLLM, LFM2.5-1.2B-Instruct, FP8) —
# started exactly as the app starts it, timed to ready, then the same cases through bench.py.
# Nothing in ElectroChat is changed. Run only when no llama-server holds the GPU.
$ErrorActionPreference = "Stop"
$script = "/mnt/d/local LLM/ElectroChat/vllm-serve.sh"
$t0 = Get-Date
$p = Start-Process -FilePath "wsl.exe" -ArgumentList @("-d", "LocalLLM", "--exec", "bash", "`"$script`"", "liquid-1.2b") -PassThru -WindowStyle Hidden `
  -RedirectStandardOutput "$PSScriptRoot\out\server-vllm.log" -RedirectStandardError "$PSScriptRoot\out\server-vllm.err.log"
$ready = $null
while (-not $ready) {
  if ($p.HasExited) { throw "vLLM exited; see out/server-vllm*.log" }
  try { if ((Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 "http://127.0.0.1:8100/health").StatusCode -eq 200) { $ready = ((Get-Date) - $t0).TotalSeconds } } catch {}
  if (((Get-Date) - $t0).TotalSeconds -gt 600) { throw "not ready after 600 s" }
  Start-Sleep -Milliseconds 250
}
"vLLM ready after {0:N1} s" -f $ready
Set-Content "$PSScriptRoot\out\vllm-ready.txt" ("{0:N2}" -f $ready)
python -I -X utf8 "$PSScriptRoot\bench.py" remote "http://127.0.0.1:8100" "liquid-1.2b" "vllm-lfm25-1.2b-fp8" "$PSScriptRoot\out\vllm-lfm25-1.2b-fp8.json"
wsl.exe -d LocalLLM --exec pkill -f "vllm serve" | Out-Null
Start-Sleep -Seconds 5
if (-not $p.HasExited) { $p.Kill() }
"vLLM stopped"
