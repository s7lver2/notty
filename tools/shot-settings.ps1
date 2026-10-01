# Lanza notty en el PERFIL DE PRUEBAS (APPDATA/LOCALAPPDATA en <repo>\.cache\appdata y
# NOTTY_PRUEBAS=1: sin registro, sin pipe de instancia única, sin atajo global), abre
# Ajustes con Ctrl+, , ejecuta los pasos pedidos sobre esa ventana y la captura por su HWND.
# Llámalo con `&` (no con `pwsh -File`) para poder recoger el PID de -KeepOpen.
#   & tools\shot-settings.ps1 -Out a.png -Steps 'click:26,250'          # Ajustes → Actualizaciones
#   & tools\shot-settings.ps1 -Out a.png -Main                          # solo la ventana principal
#   $id = & tools\shot-settings.ps1 -Out a.png -Main -KeepOpen          # deja notty abierto (PID)
#   & tools\shot-settings.ps1 -Out b.png -ProcId $id [-Main] [-KeepOpen] # otra captura de ese notty
# Pasos (en orden, sobre la ventana de Ajustes): 'click:x,y' (DIPs), 'key:VK' (decimal), 'wait:ms'.
# -MainKeys: teclas (sintaxis SendKeys) a la ventana principal antes de abrir Ajustes.
param(
  [Parameter(Mandatory)] [string]$Out,
  [string[]]$Steps = @(),
  [string[]]$MainKeys = @(),
  [switch]$Main,
  [switch]$KeepOpen,
  [int]$ProcId = 0
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target\release\notty.exe'
$prof = Join-Path $root '.cache\appdata'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
if (-not ('NottySet' -as [type])) {
  Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public class NottySet {
  public delegate bool EP(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EP p, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  // Ventana de nivel superior de ese proceso y clase (FindWindowEx no encuentra las ventanas con dueño).
  public static IntPtr FindOf(uint pid, string cls) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(128); GetClassNameW(h, sb, 128);
      if (p == pid && sb.ToString() == cls) { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
  }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool fAttach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint fl, UIntPtr ex);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
}
[NottySet]::SetProcessDPIAware() | Out-Null

function Force-Foreground([IntPtr]$h) {
  [NottySet]::ShowWindow($h, 9) | Out-Null  # SW_RESTORE
  # Un Alt suelto desbloquea SetForegroundWindow (si no, el foco se queda en otra ventana).
  [NottySet]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [NottySet]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  $fg = [NottySet]::GetForegroundWindow()
  $myTid = [NottySet]::GetCurrentThreadId()
  [uint32]$fgPid = 0
  $fgTid = [NottySet]::GetWindowThreadProcessId($fg, [ref]$fgPid)
  if ($fgTid -ne 0) { [NottySet]::AttachThreadInput($myTid, $fgTid, $true) | Out-Null }
  [NottySet]::BringWindowToTop($h) | Out-Null
  [NottySet]::SetForegroundWindow($h) | Out-Null
  if ($fgTid -ne 0) { [NottySet]::AttachThreadInput($myTid, $fgTid, $false) | Out-Null }
}
function Focus([IntPtr]$h) {
  for ($i = 0; $i -lt 10; $i++) {
    Force-Foreground $h
    Start-Sleep -Milliseconds 200
    if ([NottySet]::GetForegroundWindow() -eq $h) { return }
  }
  Write-Warning 'No se consiguió el foco de forma fiable; la captura puede salir tapada.'
}
function Find-Settings([int]$procId) { [NottySet]::FindOf([uint32]$procId, 'NottySettingsClass') }

if ($ProcId -eq 0) {
  $open = @(Get-Process notty -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe })
  if ($open.Count -gt 0) { throw "Ya hay un notty de target\release abierto (PID $($open.Id -join ', ')): no es de este script, ciérralo tú." }
  $saved = @{ APPDATA = $env:APPDATA; LOCALAPPDATA = $env:LOCALAPPDATA; NOTTY_PRUEBAS = $env:NOTTY_PRUEBAS }
  try {
    [Environment]::SetEnvironmentVariable('APPDATA', "$prof\Roaming", 'Process')
    [Environment]::SetEnvironmentVariable('LOCALAPPDATA', "$prof\Local", 'Process')
    [Environment]::SetEnvironmentVariable('NOTTY_PRUEBAS', '1', 'Process')
    $p = Start-Process -FilePath $exe -PassThru
  } finally {
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process') }
  }
} else {
  $p = Get-Process -Id $ProcId
}

try {
  for ($i = 0; $i -lt 50 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
  $mainH = $p.MainWindowHandle
  if ($mainH -eq 0) { throw 'notty no abrió ninguna ventana' }
  Start-Sleep -Milliseconds 400
  foreach ($k in $MainKeys) {
    Focus $mainH
    [System.Windows.Forms.SendKeys]::SendWait($k)
    Start-Sleep -Milliseconds 300
  }
  $target = $mainH
  if (-not $Main) {
    $h = Find-Settings $p.Id
    if ($h -eq [IntPtr]::Zero) {
      Focus $mainH
      # Engranaje de la barra de título (a 158 px del borde derecho del área cliente);
      # Ctrl+, no llega de forma fiable desde fuera.
      $cr = New-Object NottySet+RECT
      [NottySet]::GetClientRect($mainH, [ref]$cr) | Out-Null
      $gl = [IntPtr](([int]17 -shl 16) -bor [int]($cr.R - 158))
      [NottySet]::PostMessageW($mainH, 0x200, [IntPtr]0, $gl) | Out-Null
      [NottySet]::PostMessageW($mainH, 0x201, [IntPtr]1, $gl) | Out-Null
      [NottySet]::PostMessageW($mainH, 0x202, [IntPtr]0, $gl) | Out-Null
      for ($i = 0; $i -lt 100 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 100; $h = Find-Settings $p.Id }
      if ($h -eq [IntPtr]::Zero) { throw 'No se abrió la ventana de Ajustes' }
      Start-Sleep -Milliseconds 600
    }
    $scale = [NottySet]::GetDpiForWindow($h) / 96.0
    Write-Host "escala $scale"
    foreach ($s in $Steps) {
      $kind, $arg = $s -split ':', 2
      switch ($kind) {
        'click' {
          $x, $y = $arg -split ',' | ForEach-Object { [double]$_ }
          $l = [IntPtr](([int]($y * $scale) -shl 16) -bor [int]($x * $scale))
          [NottySet]::PostMessageW($h, 0x201, [IntPtr]1, $l) | Out-Null  # WM_LBUTTONDOWN
          [NottySet]::PostMessageW($h, 0x202, [IntPtr]0, $l) | Out-Null  # WM_LBUTTONUP
        }
        'key' {
          [NottySet]::PostMessageW($h, 0x100, [IntPtr][int]$arg, [IntPtr]0) | Out-Null  # WM_KEYDOWN
          [NottySet]::PostMessageW($h, 0x101, [IntPtr][int]$arg, [IntPtr]0) | Out-Null  # WM_KEYUP
        }
        'wait' { Start-Sleep -Milliseconds ([int]$arg) }
        default { throw "Paso desconocido: $s" }
      }
      Start-Sleep -Milliseconds 500
    }
    $target = $h
  }
  Focus $target
  Start-Sleep -Milliseconds 900   # que terminen la entrada de página y los fundidos (350 ms)
  $r = New-Object NottySet+RECT
  [NottySet]::GetWindowRect($target, [ref]$r) | Out-Null
  $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
  New-Item -ItemType Directory -Force (Split-Path -Parent $Out) | Out-Null
  $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Host "$($r.R - $r.L)x$($r.B - $r.T) -> $Out"
} finally {
  if ($KeepOpen) { Write-Output $p.Id }
  elseif (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
