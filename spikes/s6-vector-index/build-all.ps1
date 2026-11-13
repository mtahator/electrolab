# Spike S6: clean release build of each variant, timed (build cost is part of the decision).
param([string[]]$Variants = @("flat", "hnsw", "usearch", "sqlitevec", "lancedb"))
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
foreach ($v in $Variants) {
  Push-Location "$PSScriptRoot\$v"
  if (Test-Path target) { Remove-Item -Recurse -Force target }
  cargo fetch 2>&1 | Out-Null
  $t = Measure-Command { cargo build --release 2>&1 | Select-String -Pattern "^error" -Context 0,12 | Select-Object -First 4 | Out-Host }
  $exe = Get-ChildItem target\release\*.exe -ErrorAction SilentlyContinue | Select-Object -First 1
  $deps = (cargo tree -e normal --prefix none 2>$null | Sort-Object -Unique | Measure-Object).Count
  [pscustomobject]@{ variant = $v; ok = [bool]$exe; build_s = [math]::Round($t.TotalSeconds); exe_mb = if ($exe) { [math]::Round($exe.Length / 1MB, 1) } else { $null }; crates = $deps } | ConvertTo-Json -Compress
  Pop-Location
}
