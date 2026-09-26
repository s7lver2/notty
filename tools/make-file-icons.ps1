# Genera installer/icons/{text,config,markdown,code}.ico: una hoja de documento con
# el símbolo de la categoría y, abajo a la derecha, el logo de notty
# (tools/notty-logo.ps1) como sello. Mismo método que make-icon.ps1: GDI+, un PNG
# dibujado por tamaño y el .ico empaquetado a mano.
# Uso: pwsh tools/make-file-icons.ps1 [-PreviewDir <carpeta>]
param([string]$PreviewDir)
Add-Type -AssemblyName System.Drawing
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'notty-logo.ps1')

$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root 'installer\icons'
New-Item -ItemType Directory -Force $outDir | Out-Null

function C([int]$rgb) { [System.Drawing.Color]::FromArgb(255, ($rgb -shr 16) -band 255, ($rgb -shr 8) -band 255, $rgb -band 255) }
$page = C 0xe5e6e8       # texto principal de notty-setup, aquí como papel
$pageEdge = C 0x8a8b8e   # texto terciario
$fold = C 0xb4b5b8
$ink = C 0x46474c        # .dots apagados

function New-RoundRect([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = $r * 2
  $p.AddArc($x, $y, $d, $d, 180, 90)
  $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
  $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
  $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
  $p.CloseFigure()
  return $p
}

function New-Pen($color, [single]$w) {
  $pen = New-Object System.Drawing.Pen $color, $w
  $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
  $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
  $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
  return $pen
}

# Hoja con la esquina superior derecha doblada. Coordenadas en fracciones de $s.
function Draw-Page($g, [int]$s) {
  $l = $s * 0.16; $t = $s * 0.06; $r = $s * 0.84; $b = $s * 0.94
  $f = $s * 0.22; $rad = [Math]::Max(1.0, $s * 0.05)
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $p.AddLine($l + $rad, $t, $r - $f, $t)
  $p.AddLine($r - $f, $t, $r, $t + $f)
  $p.AddLine($r, $t + $f, $r, $b - $rad)
  $p.AddArc($r - 2 * $rad, $b - 2 * $rad, 2 * $rad, 2 * $rad, 0, 90)
  $p.AddArc($l, $b - 2 * $rad, 2 * $rad, 2 * $rad, 90, 90)
  $p.AddArc($l, $t, 2 * $rad, 2 * $rad, 180, 90)
  $p.CloseFigure()
  $brush = New-Object System.Drawing.SolidBrush $page
  $g.FillPath($brush, $p)
  $edgeW = [Math]::Max(1.0, $s / 32.0)
  $edge = New-Object System.Drawing.Pen $pageEdge, $edgeW
  $edge.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
  $g.DrawPath($edge, $p)

  $tri = New-Object System.Drawing.Drawing2D.GraphicsPath
  $tri.AddLine($r - $f, $t, $r - $f, $t + $f)
  $tri.AddLine($r - $f, $t + $f, $r, $t + $f)
  $tri.CloseFigure()
  $foldBrush = New-Object System.Drawing.SolidBrush $fold
  $g.FillPath($foldBrush, $tri)
  $g.DrawPath($edge, $tri)
  $brush.Dispose(); $edge.Dispose(); $foldBrush.Dispose(); $p.Dispose(); $tri.Dispose()
}

# Sello de notty: el logo ("n_", tools/notty-logo.ps1) en pequeño, rodeado de un
# filo del color de la hoja para que no se funda con un Explorador en tema oscuro.
function Draw-Badge($g, [int]$s) {
  $bs = [Math]::Round($s * 0.46)
  $gap = [Math]::Max(1.0, [Math]::Round($s * 0.045))
  $x = $s - $bs - $gap; $y = $s - $bs - $gap
  $hole = New-RoundRect ($x - $gap) ($y - $gap) ($bs + 2 * $gap) ($bs + 2 * $gap) ([Math]::Max(1.5, ($bs + 2 * $gap) * 0.26))
  $clear = New-Object System.Drawing.SolidBrush $page
  $g.FillPath($clear, $hole)
  Draw-NottyLogo $g $x $y $bs
  $hole.Dispose(); $clear.Dispose()
}

function Stroke-W([int]$s) { if ($s -le 20) { 1.0 } else { [Math]::Max(1.5, $s * 0.065) } }

# Los símbolos se quedan arriba a la izquierda de la hoja para no chocar con el sello.
function Draw-Text($g, [int]$s) {
  $pen = New-Pen $ink (Stroke-W $s)
  $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Flat
  $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Flat
  $l = $s * 0.28
  if ($s -le 24) { $rows = @(@(0.30, 0.70), @(0.46, 0.70), @(0.62, 0.44)) }
  else { $rows = @(@(0.26, 0.60), @(0.38, 0.72), @(0.50, 0.46), @(0.62, 0.46), @(0.74, 0.40)) }
  foreach ($row in $rows) {
    $y = [Math]::Round($s * $row[0]) + 0.5
    $g.DrawLine($pen, $l, $y, $s * $row[1], $y)
  }
  $pen.Dispose()
}

function Draw-Config($g, [int]$s) {
  $cx = $s * 0.42; $cy = $s * 0.37
  $ro = $s * 0.17; $ri = $s * 0.12
  $teeth = if ($s -le 24) { 6 } else { 8 }
  $brush = New-Object System.Drawing.SolidBrush $ink
  $gear = New-Object System.Drawing.Drawing2D.GraphicsPath
  $gear.AddEllipse($cx - $ri, $cy - $ri, $ri * 2, $ri * 2)
  $tw = [Math]::Max(1.6, $s * 0.085)
  for ($i = 0; $i -lt $teeth; $i++) {
    $m = New-Object System.Drawing.Drawing2D.Matrix
    $m.RotateAt(360.0 / $teeth * $i, (New-Object System.Drawing.PointF $cx, $cy))
    $tooth = New-Object System.Drawing.Drawing2D.GraphicsPath
    $tooth.AddRectangle((New-Object System.Drawing.RectangleF ($cx - $tw / 2), ($cy - $ro), $tw, ($ro - $ri + 1)))
    $tooth.Transform($m)
    $gear.AddPath($tooth, $false)
    $tooth.Dispose(); $m.Dispose()
  }
  $gear.FillMode = [System.Drawing.Drawing2D.FillMode]::Winding
  $g.FillPath($brush, $gear)
  $hr = [Math]::Max(1.0, $s * 0.06)
  $pageBrush = New-Object System.Drawing.SolidBrush $page
  $g.FillEllipse($pageBrush, $cx - $hr, $cy - $hr, $hr * 2, $hr * 2)
  $brush.Dispose(); $pageBrush.Dispose(); $gear.Dispose()
}

function Draw-Markdown($g, [int]$s) {
  $pen = New-Pen $ink (Stroke-W $s)
  $cx = $s * 0.42; $cy = $s * 0.37; $h = $s * 0.16
  $g.DrawLine($pen, $cx - $h * 0.30, $cy - $h, $cx - $h * 0.55, $cy + $h)
  $g.DrawLine($pen, $cx + $h * 0.55, $cy - $h, $cx + $h * 0.30, $cy + $h)
  $g.DrawLine($pen, $cx - $h, $cy - $h * 0.36, $cx + $h, $cy - $h * 0.36)
  $g.DrawLine($pen, $cx - $h * 1.05, $cy + $h * 0.36, $cx + $h * 0.95, $cy + $h * 0.36)
  $pen.Dispose()
}

function Draw-Code($g, [int]$s) {
  $pen = New-Pen $ink (Stroke-W $s)
  $cy = $s * 0.37; $h = $s * 0.13
  if ($s -le 24) { $lx = $s * 0.28; $rx = $s * 0.66; $d = $s * 0.13 }
  else { $lx = $s * 0.24; $rx = $s * 0.62; $d = $s * 0.11 }
  $g.DrawLines($pen, [System.Drawing.PointF[]]@((New-Object System.Drawing.PointF ($lx + $d), ($cy - $h)), (New-Object System.Drawing.PointF $lx, $cy), (New-Object System.Drawing.PointF ($lx + $d), ($cy + $h))))
  $g.DrawLines($pen, [System.Drawing.PointF[]]@((New-Object System.Drawing.PointF ($rx - $d), ($cy - $h)), (New-Object System.Drawing.PointF $rx, $cy), (New-Object System.Drawing.PointF ($rx - $d), ($cy + $h))))
  if ($s -gt 24) {
    $g.DrawLine($pen, $s * 0.47, $cy - $h * 1.05, $s * 0.39, $cy + $h * 1.05)
  }
  $pen.Dispose()
}

function New-Frame([int]$s, $symbol) {
  $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)
  Draw-Page $g $s
  & $symbol $g $s
  Draw-Badge $g $s
  $g.Dispose()
  return $bmp
}

function Write-Ico([string]$path, $symbol) {
  $sizes = 16, 20, 24, 32, 40, 48, 64, 128, 256
  $pngs = @()
  foreach ($s in $sizes) {
    $bmp = New-Frame $s $symbol
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngs += , $ms.ToArray()
    if ($PreviewDir) {
      New-Item -ItemType Directory -Force $PreviewDir | Out-Null
      $bmp.Save((Join-Path $PreviewDir ("{0}-{1}.png" -f [IO.Path]::GetFileNameWithoutExtension($path), $s)), [System.Drawing.Imaging.ImageFormat]::Png)
    }
    $bmp.Dispose()
  }
  $fs = [System.IO.File]::Create($path)
  $bw = New-Object System.IO.BinaryWriter $fs
  $bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)
  $offset = 6 + (16 * $sizes.Count)
  for ($i = 0; $i -lt $sizes.Count; $i++) {
    $b = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $bw.Write([byte]$b); $bw.Write([byte]$b)
    $bw.Write([byte]0); $bw.Write([byte]0)
    $bw.Write([uint16]1); $bw.Write([uint16]32)
    $bw.Write([uint32]$pngs[$i].Length)
    $bw.Write([uint32]$offset)
    $offset += $pngs[$i].Length
  }
  foreach ($pb in $pngs) { $bw.Write($pb) }
  $bw.Flush(); $bw.Close(); $fs.Close()
  "ok -> $path ($((Get-Item $path).Length) bytes)"
}

Write-Ico (Join-Path $outDir 'text.ico') ${function:Draw-Text}
Write-Ico (Join-Path $outDir 'config.ico') ${function:Draw-Config}
Write-Ico (Join-Path $outDir 'markdown.ico') ${function:Draw-Markdown}
Write-Ico (Join-Path $outDir 'code.ico') ${function:Draw-Code}
