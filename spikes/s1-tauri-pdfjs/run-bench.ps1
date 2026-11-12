# Spike S1: run every measurement on the chosen fixture papers. Each run is a fresh app process.
# Usage (from this folder): pwsh ./run-bench.ps1
$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot\..\.."
$exe = "$PSScriptRoot\src-tauri\target\release\s1-tauri-pdfjs.exe"
$out = "$PSScriptRoot\out"
New-Item -ItemType Directory -Force $out | Out-Null
$paper = { param($f) if (Test-Path "$root\.fixtures\papers\$f") { "$root\.fixtures\papers\$f" } else { "$root\fixtures\papers\$f" } }

$runs = @(
  # mode,   file,                                   pages
  @("open",   "arxiv-2106.03711v2.pdf",   0),   # 31 p, heaviest pages
  @("open",   "plos-pone-0232683.pdf",    0),   # 28 p, journal layout
  @("open",   "arxiv-2111.07691v2.pdf",   0),   # 38 p
  @("open",   "arxiv-2406.19831v2.pdf",   0),   # 24 p, figure-heavy
  @("open",   "naca-report-1135.pdf",     0),   # scanned images
  @("open",   "fawiki-artificial-neural-network.pdf", 0),
  @("scroll", "arxiv-2601.05155v2.pdf", 100),   # thesis, first 100 pages
  @("scroll", "arxiv-2511.21915v1.pdf", 100),   # thesis, first 100 pages
  @("scroll", "arxiv-2106.03711v2.pdf",  31),   # heaviest pages
  @("scroll", "naca-report-1135.pdf",    70),   # scanned images
  @("jump",   "arxiv-2511.21915v1.pdf",   0),
  @("jump",   "naca-report-1135.pdf",     0)
)
foreach ($r in $runs) {
  $mode, $file, $n = $r
  $pdf = & $paper $file
  $json = "$out\$mode-$([IO.Path]::GetFileNameWithoutExtension($file)).json"
  Remove-Item $json -ErrorAction SilentlyContinue
  $args = @("--bench", $mode, "--pdf", "`"$pdf`"", "--out", "`"$json`"")
  if ($n -gt 0) { $args += @("--pages", "$n") }
  $p = Start-Process -FilePath $exe -ArgumentList $args -PassThru
  if (-not $p.WaitForExit(300000)) { $p.Kill(); Write-Output "TIMEOUT $mode $file"; continue }
  Write-Output "done $mode $file"
  Start-Sleep -Seconds 2
}
