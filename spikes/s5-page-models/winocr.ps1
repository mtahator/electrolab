# Spike S5: Windows' built-in OCR (Windows.Media.Ocr) on page images. Run with Windows PowerShell 5.1
# (it can load WinRT types directly). Prints one JSON line per image: milliseconds and recognised text.
# Usage: powershell.exe -NoProfile -File winocr.ps1 <image>...
param([Parameter(ValueFromRemainingArguments)] [string[]]$Images)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Runtime.WindowsRuntime
[void][Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
[void][Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
[void][Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics.Imaging, ContentType = WindowsRuntime]
$asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' } | Select-Object -First 1
function Await($op, [Type]$t) { $task = $asTask.MakeGenericMethod($t).Invoke($null, @($op)); $task.Wait(); $task.Result }
$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
foreach ($img in $Images) {
  $file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync((Resolve-Path $img).Path)) ([Windows.Storage.StorageFile])
  $stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
  $decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
  $bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
  $sw.Stop()
  $text = ($result.Lines | ForEach-Object { $_.Text }) -join "`n"
  [pscustomobject]@{ image = (Split-Path $img -Leaf); ms = $sw.ElapsedMilliseconds; lines = $result.Lines.Count; text = $text } | ConvertTo-Json -Compress
  $stream.Dispose()
}
