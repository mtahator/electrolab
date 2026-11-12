# Spike S3: every llama.cpp backend × model. CUDA runs all cases (quality + speed);
# Vulkan and CPU run 15 cases (speed; quality does not depend on the backend).
$ErrorActionPreference = "Continue"
$store = "D:\local LLM\LLM-Models\gguf"
$models = @(
  @("lfm25-1.2b-q8", "LFM2.5-1.2B-Instruct-Q8_0.gguf"),
  @("lfm25-1.2b-q4", "LFM2.5-1.2B-Instruct-Q4_K_M.gguf"),
  @("qwen3-4b-q4",   "Qwen3-4B-Instruct-2507-Q4_K_M.gguf")
)
foreach ($backend in "cuda", "vulkan", "cpu") {
  foreach ($m in $models) {
    $label = "$backend-$($m[0])"
    $extra = if ($backend -eq "cuda") { @() } else { @("--cases", "15") }
    Write-Output "== $label"
    python -I -X utf8 "$PSScriptRoot\bench.py" llama $backend (Join-Path $store $m[1]) $label "$PSScriptRoot\out\$label.json" @extra
    if ($LASTEXITCODE -ne 0) { Write-Output "FAILED $label"; Get-Content "$PSScriptRoot\out\server-$backend.log" -Tail 8 }
    Start-Sleep -Seconds 3
  }
}
