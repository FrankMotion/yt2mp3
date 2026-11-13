# Builds res/icon.ico from the raven-and-moon artwork:
#   res/icon-source.png        full detail, used for 48-256 px
#   res/icon-source-small.png  smooth bright moon, used for 16-32 px
#                              (moon spots turn into noise at tiny sizes)
# Each size gets rounded corners; all sizes are packed as PNGs into one .ico.
Add-Type -AssemblyName System.Drawing
$res = Join-Path (Split-Path $PSScriptRoot -Parent) 'res'

function Render([string]$source, [int]$s) {
  $img = [System.Drawing.Image]::FromFile($source)
  # 1. scale down with good resampling
  $scaled = New-Object System.Drawing.Bitmap $s, $s
  $g = [System.Drawing.Graphics]::FromImage($scaled)
  $g.InterpolationMode = 'HighQualityBicubic'
  $g.PixelOffsetMode = 'HighQuality'
  $g.DrawImage($img, 0, 0, $s, $s)
  $g.Dispose(); $img.Dispose()
  # 2. cut rounded corners (anti-aliased) onto a transparent canvas
  $out = New-Object System.Drawing.Bitmap $s, $s
  $g = [System.Drawing.Graphics]::FromImage($out)
  $g.SmoothingMode = 'AntiAlias'
  $g.PixelOffsetMode = 'HighQuality'
  $g.Clear([System.Drawing.Color]::Transparent)
  $r = [float]([math]::Max(2, $s * 0.22))
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $p.AddArc(0, 0, 2 * $r, 2 * $r, 180, 90)
  $p.AddArc($s - 2 * $r, 0, 2 * $r, 2 * $r, 270, 90)
  $p.AddArc($s - 2 * $r, $s - 2 * $r, 2 * $r, 2 * $r, 0, 90)
  $p.AddArc(0, $s - 2 * $r, 2 * $r, 2 * $r, 90, 90)
  $p.CloseFigure()
  $brush = New-Object System.Drawing.TextureBrush $scaled
  $g.FillPath($brush, $p)
  $g.Dispose(); $brush.Dispose(); $scaled.Dispose()
  $ms = New-Object System.IO.MemoryStream
  $out.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); $out.Dispose()
  , $ms.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$pngs = foreach ($s in $sizes) {
  $src = if ($s -le 32) { Join-Path $res 'icon-source-small.png' } else { Join-Path $res 'icon-source.png' }
  , (Render $src $s)
}

# .ico container: header, one directory entry per size, then the PNG data
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
[IO.File]::WriteAllBytes((Join-Path $res 'icon.ico'), $out.ToArray())
"Icon: $(Join-Path $res 'icon.ico') ($([math]::Round($out.Length/1KB)) KB)"
