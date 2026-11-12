# Spike S1: real mouse input. Starts the app in "select" mode, reads the targets it reports
# (CSS pixels in the web view), converts them to screen pixels, and sends a genuine
# double-click on a word and a drag across a line with Windows SendInput-level mouse events.
# The app then reports what the browser actually selected.
# Usage: pwsh ./real-input.ps1 <pdf-path> <out-json>
param([Parameter(Mandatory)] [string]$Pdf, [Parameter(Mandatory)] [string]$Out)
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class Win {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  public const uint DOWN = 0x0002, UP = 0x0004;
}
"@
# Physical pixels everywhere: per-monitor DPI aware (v2).
[void][Win]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))

$exe = "$PSScriptRoot\src-tauri\target\release\s1-tauri-pdfjs.exe"
$targets = [IO.Path]::ChangeExtension($Out, "targets.json")
Remove-Item $Out, $targets -ErrorAction SilentlyContinue
$p = Start-Process -FilePath $exe -ArgumentList @("--bench", "select", "--pdf", "`"$Pdf`"", "--out", "`"$Out`"") -PassThru
$t = [DateTime]::Now
while (-not (Test-Path $targets)) { if (([DateTime]::Now - $t).TotalSeconds -gt 60) { throw "no targets" }; Start-Sleep -Milliseconds 200 }
$tg = Get-Content $targets | ConvertFrom-Json
$p.Refresh()
$h = $p.MainWindowHandle
[void][Win]::SetForegroundWindow($h)
Start-Sleep -Milliseconds 300
$origin = New-Object Win+POINT
[void][Win]::ClientToScreen($h, [ref]$origin)
$toScreen = { param($xy) @([int]($origin.X + $xy[0] * $tg.dpr), [int]($origin.Y + $xy[1] * $tg.dpr)) }

# 1. double-click the word
$w = & $toScreen $tg.word
[void][Win]::SetCursorPos($w[0], $w[1]); Start-Sleep -Milliseconds 150
foreach ($i in 1..2) { [Win]::mouse_event([Win]::DOWN, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 30; [Win]::mouse_event([Win]::UP, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60 }
Start-Sleep -Milliseconds 800   # longer than the double-click interval

# 2. drag across the line
$a = & $toScreen $tg.drag_start; $b = & $toScreen $tg.drag_end
[void][Win]::SetCursorPos($a[0], $a[1]); Start-Sleep -Milliseconds 150
[Win]::mouse_event([Win]::DOWN, 0, 0, 0, [UIntPtr]::Zero)
foreach ($k in 1..20) { [void][Win]::SetCursorPos([int]($a[0] + ($b[0] - $a[0]) * $k / 20), [int]($a[1] + ($b[1] - $a[1]) * $k / 20)); Start-Sleep -Milliseconds 15 }
[Win]::mouse_event([Win]::UP, 0, 0, 0, [UIntPtr]::Zero)

if (-not $p.WaitForExit(40000)) { $p.Kill(); throw "app did not report" }
Get-Content $Out
