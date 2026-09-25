# Genera assets/notty.ico (icono "trazo + punto") a partir de GDI+, con una imagen
# PNG dibujada a mano por tamaño (no un solo master reescalado) para que el trazo
# no se quede demasiado fino en 16/20/24px. Uso: pwsh tools/make-icon.ps1
Add-Type -AssemblyName System.Drawing
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root 'assets'
New-Item -ItemType Directory -Force $outDir | Out-Null

$bg = [System.Drawing.Color]::FromArgb(255, 0x1d, 0x1e, 0x1f)      # --surface (oscuro)
$accent = [System.Drawing.Color]::FromArgb(255, 0x4a, 0xa8, 0xf5)   # acento, un pelín más saturado que --accent para que lea bien en 16px

function New-Frame([int]$s) {
  $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)

  $radius = [Math]::Max(2.0, $s * 0.22)
  $rect = New-Object System.Drawing.RectangleF 0, 0, $s, $s
  $path = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = $radius * 2
  $path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
  $path.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
  $path.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
  $path.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
  $path.CloseFigure()
  $bgBrush = New-Object System.Drawing.SolidBrush $bg
  $g.FillPath($bgBrush, $path)

  # Trazo diagonal (esquina inferior-izq a superior-der) + punto, con suelo mínimo
  # de grosor/radio para que siga siendo legible en los tamaños más pequeños.
  $strokeW = [Math]::Max(2.0, $s * 0.135)
  $pen = New-Object System.Drawing.Pen $accent, $strokeW
  $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
  $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
  $x1 = $s * 0.28; $y1 = $s * 0.68
  $x2 = $s * 0.60; $y2 = $s * 0.36
  $g.DrawLine($pen, $x1, $y1, $x2, $y2)

  $dotR = [Math]::Max(1.4, $s * 0.075)
  $dotCx = $s * 0.72; $dotCy = $s * 0.27
  $dotBrush = New-Object System.Drawing.SolidBrush $accent
  $g.FillEllipse($dotBrush, $dotCx - $dotR, $dotCy - $dotR, $dotR * 2, $dotR * 2)

  $g.Dispose(); $bgBrush.Dispose(); $pen.Dispose(); $dotBrush.Dispose(); $path.Dispose()
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
