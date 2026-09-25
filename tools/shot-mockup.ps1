# Genera las capturas de referencia de la maqueta (920x600, escala 1) en docs/mockups/ref/.
# Uso: pwsh tools/shot-mockup.ps1
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root 'docs\mockups\ref'
New-Item -ItemType Directory -Force $out | Out-Null
$chrome = @("$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
            "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
$html = (Join-Path $root 'docs\mockups\notty-ui.html') -replace '\\', '/'
$tmp = Join-Path $env:TEMP 'notty-shot'
New-Item -ItemType Directory -Force $tmp | Out-Null

$shots = [ordered]@{
  'moderna-oscuro'      = 'shot=default'
  'moderna-claro'       = 'shot=default&theme=light'
  'sucio'               = 'shot=dirty'
  'clasica'             = 'shot=default&preset=clasica'
  'zen'                 = 'shot=default&preset=zen'
  'clickme'             = 'shot=perm'
  'ruta-sugerencias'    = 'shot=open&value=~\Documentos\p'
  'ruta-nueva'          = 'shot=save&value=~\Documentos\facturas\octubre'
  'buscar'              = 'shot=find&q=Juan'
  'reemplazar'          = 'shot=replace&q=Juan&rep=Pedro'
  'vim-normal'          = 'shot=vim'
  'vim-insert'          = 'shot=vim&mode=INSERT'
  'raw'                 = 'shot=bin'
  'raw-solo-lectura'    = 'shot=binro'
  'ajustes'             = 'shot=settings'
}
foreach ($name in $shots.Keys) {
  $full = Join-Path $tmp "$name.png"
  $url = "file:///$html`?$($shots[$name])"
  Start-Process $chrome -ArgumentList '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=1',
    '--window-size=1000,700', "--user-data-dir=$tmp\profile", "--screenshot=$full", "`"$url`"" -Wait
  $src = [System.Drawing.Bitmap]::FromFile($full)
  # La ventana de la maqueta queda centrada en el escritorio: 920x600 en (40, 28). Ajustes flota encima.
  $rect = if ($name -eq 'ajustes') { New-Object System.Drawing.Rectangle 0, 0, 1000, 656 } else { New-Object System.Drawing.Rectangle 40, 28, 920, 600 }
  $crop = $src.Clone($rect, $src.PixelFormat)
  $crop.Save((Join-Path $out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $crop.Dispose(); $src.Dispose()
  "ok  $name"
}
