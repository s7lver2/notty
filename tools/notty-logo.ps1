# El logo de notty ("n_" sobre un cuadrado redondeado), dibujado con GDI+. Lo usan
# make-icon.ps1 (assets/notty.ico) y make-file-icons.ps1 (el sello de los iconos de
# archivo). Misma geometría que `LOGO_N`/`LOGO_CARET` en crates/notty-ui/src/render/extra.rs
# (lienzo de 24x24).
Add-Type -AssemblyName System.Drawing

$LogoBg = [System.Drawing.Color]::FromArgb(255, 0x73, 0xb6, 0xfa)   # --accent (oscuro)
$LogoFg = [System.Drawing.Color]::FromArgb(255, 0x17, 0x18, 0x1a)   # --on-accent

function Draw-NottyLogo($g, [single]$x, [single]$y, [single]$s) {
  $k = $s / 24.0
  $r = [Math]::Max(1.5, $s * 0.22)
  $d = $r * 2
  $box = New-Object System.Drawing.Drawing2D.GraphicsPath
  $box.AddArc($x, $y, $d, $d, 180, 90)
  $box.AddArc($x + $s - $d, $y, $d, $d, 270, 90)
  $box.AddArc($x + $s - $d, $y + $s - $d, $d, $d, 0, 90)
  $box.AddArc($x, $y + $s - $d, $d, $d, 90, 90)
  $box.CloseFigure()
  $bg = New-Object System.Drawing.SolidBrush $LogoBg
  $g.FillPath($bg, $box)

  # En 16-24px el trazo de 2.4 unidades se quedaría en ~1.6px: se engorda un poco.
  $w = if ($s -le 24) { [Math]::Max(1.8, 2.8 * $k) } else { 2.4 * $k }
  $pen = New-Object System.Drawing.Pen $LogoFg, $w
  $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
  $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
  $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round

  $n = New-Object System.Drawing.Drawing2D.GraphicsPath
  $n.StartFigure()
  $n.AddLine($x + 5.6 * $k, $y + 8.4 * $k, $x + 5.6 * $k, $y + 17 * $k)
  $n.StartFigure()
  $n.AddLine($x + 5.6 * $k, $y + 17 * $k, $x + 5.6 * $k, $y + 12.3 * $k)
  $n.AddArc($x + 5.6 * $k, $y + 8.4 * $k, 7.8 * $k, 7.8 * $k, 180, 180)
  $n.AddLine($x + 13.4 * $k, $y + 12.3 * $k, $x + 13.4 * $k, $y + 17 * $k)
  $g.DrawPath($pen, $n)
  $g.DrawLine($pen, $x + 16.4 * $k, $y + 17 * $k, $x + 19 * $k, $y + 17 * $k)

  $box.Dispose(); $bg.Dispose(); $pen.Dispose(); $n.Dispose()
}
