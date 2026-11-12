# Draws the plain app icon (dark tile, white down arrow) and packs
# 16-256 px PNGs into res/icon.ico.
Add-Type -AssemblyName System.Drawing

function Draw([int]$s) {
  $bmp = New-Object System.Drawing.Bitmap $s, $s
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = 'AntiAlias'
  $g.Clear([System.Drawing.Color]::Transparent)
  $r = [float]($s * 0.22)
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $p.AddArc(0, 0, 2 * $r, 2 * $r, 180, 90)
  $p.AddArc($s - 2 * $r - 1, 0, 2 * $r, 2 * $r, 270, 90)
  $p.AddArc($s - 2 * $r - 1, $s - 2 * $r - 1, 2 * $r, 2 * $r, 0, 90)
  $p.AddArc(0, $s - 2 * $r - 1, 2 * $r, 2 * $r, 90, 90)
  $p.CloseFigure()
  $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 32, 32, 34))), $p)
  $w = [float][math]::Max(1.5, $s * 0.085)
  $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), $w
  $pen.StartCap = 'Round'; $pen.EndCap = 'Round'; $pen.LineJoin = 'Round'
  $cx = $s / 2
  $g.DrawLine($pen, $cx, $s * 0.24, $cx, $s * 0.64)                       # shaft
  $g.DrawLines($pen, [System.Drawing.PointF[]]@(
      (New-Object System.Drawing.PointF ($s * 0.33), ($s * 0.48)),
      (New-Object System.Drawing.PointF $cx, ($s * 0.65)),
      (New-Object System.Drawing.PointF ($s * 0.67), ($s * 0.48))))       # head
  $g.DrawLine($pen, $s * 0.30, $s * 0.78, $s * 0.70, $s * 0.78)          # base line
  $g.Dispose()
  $ms = New-Object System.IO.MemoryStream
  $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  , $ms.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$pngs = $sizes | ForEach-Object { , (Draw $_) }
$out = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter $out
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
  $s = $sizes[$i]; $len = $pngs[$i].Length
  $bw.Write([byte]($s % 256)); $bw.Write([byte]($s % 256))
  $bw.Write([byte]0); $bw.Write([byte]0)
  $bw.Write([uint16]1); $bw.Write([uint16]32)
  $bw.Write([uint32]$len); $bw.Write([uint32]$offset)
  $offset += $len
}
foreach ($png in $pngs) { $bw.Write([byte[]]$png) }
$bw.Flush()
$dest = Join-Path (Split-Path $PSScriptRoot -Parent) 'res\icon.ico'
[IO.File]::WriteAllBytes($dest, $out.ToArray())
"Icon: $dest ($([math]::Round($out.Length/1KB)) KB)"
