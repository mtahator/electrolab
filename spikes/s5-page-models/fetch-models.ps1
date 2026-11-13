# Spike S5: download the page models in models.json into the user's model store,
# pinned to a Hugging Face commit; large files verified by SHA-256 (small config files are pinned by commit).
# Usage: pwsh ./fetch-models.ps1 [-Store "D:\local LLM\LLM-Models\page-models"]
param([string]$Store = "D:\local LLM\LLM-Models\page-models")
$ErrorActionPreference = "Stop"
foreach ($m in (Get-Content "$PSScriptRoot\models.json" -Raw | ConvertFrom-Json)) {
  $dir = Join-Path $Store $m.id
  New-Item -ItemType Directory -Force $dir | Out-Null
  $dest = Join-Path $dir $m.file
  $ok = { param($p) if (-not $m.sha256) { Test-Path $p } else { (Test-Path $p) -and ((Get-FileHash $p -Algorithm SHA256).Hash.ToLower() -eq $m.sha256) } }
  if (& $ok $dest) { "cached  $($m.id)/$($m.file)"; continue }
  $url = "https://huggingface.co/$($m.repo)/resolve/$($m.rev)/$($m.file)"
  & curl.exe --fail --location --silent --show-error --proto "=https" --retry 3 --user-agent "electropaper-spike" --output "$dest.partial" $url
  if ($LASTEXITCODE -ne 0) { throw "download failed: $url" }
  if ($m.sha256 -and (Get-FileHash "$dest.partial" -Algorithm SHA256).Hash.ToLower() -ne $m.sha256) { Remove-Item "$dest.partial"; throw "hash mismatch: $($m.file)" }
  Move-Item -Force "$dest.partial" $dest
  "fetched $($m.id)/$($m.file)"
}
