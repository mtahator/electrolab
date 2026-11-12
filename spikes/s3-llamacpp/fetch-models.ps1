# Spike S3: download the GGUF models listed in models.json into the user's model store,
# pinned to a Hugging Face commit and verified by SHA-256. Skips files already verified.
# Usage: pwsh ./fetch-models.ps1 [-Store "D:\local LLM\LLM-Models\gguf"]
param([string]$Store = "D:\local LLM\LLM-Models\gguf")
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $Store | Out-Null
foreach ($m in (Get-Content "$PSScriptRoot\models.json" -Raw | ConvertFrom-Json)) {
  $dest = Join-Path $Store $m.file
  if ((Test-Path $dest) -and ((Get-FileHash $dest -Algorithm SHA256).Hash.ToLower() -eq $m.sha256)) { "cached  $($m.file)"; continue }
  $url = "https://huggingface.co/$($m.repo)/resolve/$($m.rev)/$($m.file)"
  $part = "$dest.partial"
  & curl.exe --fail --location --silent --show-error --proto "=https" --retry 3 --continue-at - --user-agent "electropaper-spike" --output $part $url
  if ($LASTEXITCODE -ne 0) { throw "download failed: $url" }
  $got = (Get-FileHash $part -Algorithm SHA256).Hash.ToLower()
  if ($got -ne $m.sha256) { Remove-Item $part; throw "hash mismatch for $($m.file): $got" }
  Move-Item -Force $part $dest
  "fetched $($m.file)"
}
