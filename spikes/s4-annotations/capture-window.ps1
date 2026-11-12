# Capture one window (found by title) to a PNG: bring it to the front, copy its screen rectangle.
# Used to look at the annotated test PDF in Edge (Adobe's PDF engine). Only that window is captured.
# Usage: pwsh ./capture-window.ps1 -TitleLike "annotated.pdf*Edge*" -Out out/edge.png
param([Parameter(Mandatory)] [string]$TitleLike, [Parameter(Mandatory)] [string]$Out)
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
}
"@
[void][W]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))
$p = Get-Process | Where-Object { $_.MainWindowTitle -like $TitleLike } | Select-Object -First 1
if (-not $p) { throw "no window titled like '$TitleLike'" }
[void][W]::ShowWindow($p.MainWindowHandle, 9); [void][W]::SetForegroundWindow($p.MainWindowHandle)
Start-Sleep -Milliseconds 1500
$r = New-Object W+RECT
[void][W]::DwmGetWindowAttribute($p.MainWindowHandle, 9, [ref]$r, 16)   # extended frame bounds
$bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"saved $Out ($($bmp.Width)x$($bmp.Height)) from '$($p.MainWindowTitle)'"
