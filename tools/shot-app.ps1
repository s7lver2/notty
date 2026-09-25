# Abre notty.exe, opcionalmente le manda teclas, y guarda una captura de la ventana (escala 1).
# Uso: pwsh tools/shot-app.ps1 -Out shot.png [-File ruta] [-Keys '^f' , 'Juan'] [-Exe target\release\notty.exe]
# -Keys usa la sintaxis de SendKeys: ^ = Ctrl, + = Shift, % = Alt, {TAB}, {ENTER}, {ESC}...
param(
  [Parameter(Mandatory)] [string]$Out,
  [string]$File,
  [string[]]$Keys = @(),
  [string]$Exe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\notty.exe')
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public class NottyShot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
[NottyShot]::SetProcessDPIAware() | Out-Null
$p = if ($File) { Start-Process -FilePath $Exe -ArgumentList "`"$File`"" -PassThru } else { Start-Process -FilePath $Exe -PassThru }
try {
  for ($i = 0; $i -lt 50 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
  $h = $p.MainWindowHandle
  if ($h -eq 0) { throw 'notty no abrió ninguna ventana' }
  [NottyShot]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 400
  foreach ($k in $Keys) { [System.Windows.Forms.SendKeys]::SendWait($k); Start-Sleep -Milliseconds 150 }
  Start-Sleep -Milliseconds 400
  $r = New-Object NottyShot+RECT
  [NottyShot]::GetWindowRect($h, [ref]$r) | Out-Null
  $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
  $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
  "$($bmp.Width)x$($bmp.Height) -> $Out"
} finally {
  if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
