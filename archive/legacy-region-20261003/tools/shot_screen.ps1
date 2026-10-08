# 真实屏幕抓图（含 Rgn 裁剪后的实际观感）：截取圆心附近区域并存 PNG
param([int]$Half = 300, [string]$Out = "E:\tools\ring-dock\target\shot_screen.png")
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$w = $bounds.Width; $h = $bounds.Height
$cx = [int]($w / 2); $cy = [int]($h / 2)
$x0 = [Math]::Max(0, $cx - $Half); $y0 = [Math]::Max(0, $cy - $Half)
$bw = [Math]::Min(2 * $Half, $w - $x0); $bh = [Math]::Min(2 * $Half, $h - $y0)
$bmp = New-Object System.Drawing.Bitmap($bw, $bh)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x0, $y0, 0, 0, (New-Object System.Drawing.Size($bw, $bh)))
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Host "屏幕截图：$Out ($bw x $bh, 原点 $x0,$y0)"
