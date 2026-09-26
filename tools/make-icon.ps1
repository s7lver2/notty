# Genera assets/notty.ico (el logo "n_", ver tools/notty-logo.ps1) con GDI+: una
# imagen PNG dibujada por tamaño (no un solo master reescalado) para que el trazo no
# se quede demasiado fino en 16/20/24px. Uso: pwsh tools/make-icon.ps1
Add-Type -AssemblyName System.Drawing
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'notty-logo.ps1')

$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root 'assets'
New-Item -ItemType Directory -Force $outDir | Out-Null

function New-Frame([int]$s) {
  $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)
  Draw-NottyLogo $g 0 0 $s
  $g.Dispose()
  return $bmp
}

$sizes = 16, 20, 24, 32, 40, 48, 64, 128, 256
$frames = @{}
foreach ($s in $sizes) { $frames[$s] = New-Frame $s }

# Empaqueta el .ico a mano: ICONDIR + ICONDIRENTRY[] + PNG por tamaño (formato que
# Windows Vista+ entiende para iconos de más de 8bpp / mayores de 256px).
$icoPath = "$outDir\notty.ico"
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter $fs
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)  # ICONDIR

$pngBytesList = @()
foreach ($s in $sizes) {
  $ms = New-Object System.IO.MemoryStream
  $frames[$s].Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
  $pngBytesList += , $ms.ToArray()
}

$offset = 6 + (16 * $sizes.Count)
for ($i = 0; $i -lt $sizes.Count; $i++) {
  $s = $sizes[$i]
  $b = if ($s -ge 256) { 0 } else { $s }
  $bw.Write([byte]$b); $bw.Write([byte]$b)      # width, height (0 = 256)
  $bw.Write([byte]0); $bw.Write([byte]0)        # color count, reserved
  $bw.Write([uint16]1); $bw.Write([uint16]32)   # planes, bpp
  $bw.Write([uint32]$pngBytesList[$i].Length)
  $bw.Write([uint32]$offset)
  $offset += $pngBytesList[$i].Length
}
foreach ($pb in $pngBytesList) { $bw.Write($pb) }
$bw.Flush(); $bw.Close(); $fs.Close()

foreach ($s in $sizes) { $frames[$s].Dispose() }
"ok -> $icoPath ($((Get-Item $icoPath).Length) bytes)"
