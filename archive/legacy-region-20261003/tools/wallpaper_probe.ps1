# 壁纸层抓取探针：PrintWindow(PW_RENDERFULLCONTENT) 能否拿到“不含其他应用窗口”的壁纸？
# 若可以 → 假透明方案可以随时刷新底图（无闪烁、无污染）
Add-Type -AssemblyName System.Drawing
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class WP {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left, top, right, bottom; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT rc);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [DllImport("gdi32.dll")]  public static extern IntPtr CreateCompatibleDC(IntPtr dc);
  [DllImport("gdi32.dll")]  public static extern IntPtr CreateCompatibleBitmap(IntPtr dc, int w, int h);
  [DllImport("gdi32.dll")]  public static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
  [DllImport("gdi32.dll")]  public static extern bool DeleteObject(IntPtr o);
  [DllImport("gdi32.dll")]  public static extern bool DeleteDC(IntPtr dc);
  [DllImport("gdi32.dll")]  public static extern bool BitBlt(IntPtr dst, int x, int y, int w, int h, IntPtr src, int sx, int sy, uint rop);
  [DllImport("gdi32.dll")]  public static extern int GetDIBits(IntPtr dc, IntPtr bmp, uint s, uint l, byte[] buf, ref BI bi, uint u);
  public struct BIH { public uint s; public int w, h; public ushort p, b; public uint c, si; public int x, y; public uint cu, ci; }
  public struct BI { public BIH h; public uint col; }

  public static string ClassOf(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }
  public static IntPtr FindAny(string cls) {
    IntPtr found = IntPtr.Zero;
    EnumProc childCb = (h, l) => { if (ClassOf(h) == cls) { found = h; return false; } return true; };
    EnumProc topCb = (h, l) => {
      if (ClassOf(h) == cls) { found = h; return false; }
      EnumChildWindows(h, childCb, IntPtr.Zero);
      return found == IntPtr.Zero;
    };
    EnumWindows(topCb, IntPtr.Zero);
    return found;
  }
  public static string TitleOf(IntPtr h) { var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256); return sb.ToString(); }

  public static IntPtr[] TopLevels() {
    System.Collections.Generic.List<IntPtr> list = new System.Collections.Generic.List<IntPtr>();
    EnumProc cb = (h, l) => { list.Add(h); return true; };
    EnumWindows(cb, IntPtr.Zero);
    return list.ToArray();
  }
  public static IntPtr[] Children(IntPtr p) {
    System.Collections.Generic.List<IntPtr> list = new System.Collections.Generic.List<IntPtr>();
    EnumProc cb = (h, l) => { list.Add(h); return true; };
    EnumChildWindows(p, cb, IntPtr.Zero);
    return list.ToArray();
  }

  // PrintWindow 抓取指定窗口自身渲染 → 24bpp 像素
  public static byte[] PrintShot(IntPtr h, out int w, out int ht) {
    RECT rc; GetWindowRect(h, out rc);
    w = rc.right - rc.left; ht = rc.bottom - rc.top;
    IntPtr wdc = GetDC(IntPtr.Zero), memdc = CreateCompatibleDC(wdc);
    IntPtr bmp = CreateCompatibleBitmap(wdc, w, ht);
    IntPtr old = SelectObject(memdc, bmp);
    bool ok = PrintWindow(h, memdc, 0x00000002); // PW_RENDERFULLCONTENT
    var bi = new BI(); bi.h.s = 40; bi.h.w = w; bi.h.h = -ht; bi.h.p = 1; bi.h.b = 24;
    byte[] buf = new byte[w * ht * 3 + w * 4];
    GetDIBits(memdc, bmp, 0, (uint)ht, buf, ref bi, 0);
    SelectObject(memdc, old); DeleteObject(bmp); DeleteDC(memdc); ReleaseDC(IntPtr.Zero, wdc);
    return ok ? buf : null;
  }

  // 屏幕合成抓取（含所有上层窗口）
  public static byte[] ScreenShot(int x, int y, int w, int ht) {
    IntPtr wdc = GetDC(IntPtr.Zero), memdc = CreateCompatibleDC(wdc);
    IntPtr bmp = CreateCompatibleBitmap(wdc, w, ht);
    IntPtr old = SelectObject(memdc, bmp);
    BitBlt(memdc, 0, 0, w, ht, wdc, x, y, 0x00CC0020);
    var bi = new BI(); bi.h.s = 40; bi.h.w = w; bi.h.h = -ht; bi.h.p = 1; bi.h.b = 24;
    byte[] buf = new byte[w * ht * 3 + w * 4];
    GetDIBits(memdc, bmp, 0, (uint)ht, buf, ref bi, 0);
    SelectObject(memdc, old); DeleteObject(bmp); DeleteDC(memdc); ReleaseDC(IntPtr.Zero, wdc);
    return buf;
  }

  public static void SaveBmp(byte[] pix, int w, int h, string path) {
    int stride = ((w * 24 + 31) / 32) * 4;
    int dataSize = stride * h;
    using (var fs = new System.IO.FileStream(path, System.IO.FileMode.Create))
    using (var bw = new System.IO.BinaryWriter(fs)) {
      bw.Write((ushort)0x4D42); bw.Write(14 + 40 + dataSize); bw.Write(0); bw.Write(14 + 40);
      bw.Write((uint)40); bw.Write(w); bw.Write(h);
      bw.Write((ushort)1); bw.Write((ushort)24);
      bw.Write(0u); bw.Write((uint)dataSize); bw.Write(0); bw.Write(0); bw.Write(0u); bw.Write(0u);
      byte[] line = new byte[stride];
      for (int y = h - 1; y >= 0; y--) {
        Array.Copy(pix, y * stride, line, 0, stride);
        bw.Write(line);
      }
    }
  }
}
"@
Add-Type -TypeDefinition $src

# 枚举所有顶层窗口，找桌面壁纸相关类名
$tops = [WP]::TopLevels()
Write-Host "--- 顶层窗口 ---"
foreach ($t in $tops) {
  $c = [WP]::ClassOf($t)
  if ($c -match "Progman|WorkerW") {
    $rc = New-Object WP+RECT
    [WP]::GetWindowRect($t, [ref]$rc) | Out-Null
    Write-Host ("{0}  hwnd={1}  rect={2},{3},{4},{5}  title='{6}'" -f $c, $t, $rc.left, $rc.top, $rc.right, $rc.bottom, [WP]::TitleOf($t))
    foreach ($ch in [WP]::Children($t)) {
      Write-Host ("    child: {0} hwnd={1}" -f [WP]::ClassOf($ch), $ch)
    }
  }
}

# 选择目标：壁纸相关窗口优先（Wallpaper Engine 的 DX11 窗口 / WorkerW / Progman）
$target = [IntPtr]::Zero; $name = ""
foreach ($want in @("WPEDesktopDX11Window", "WorkerW", "Progman")) {
  foreach ($t in $tops) {
    if ([WP]::ClassOf($t) -eq $want) {
      $rc2 = New-Object WP+RECT
      [WP]::GetWindowRect($t, [ref]$rc2) | Out-Null
      if (($rc2.right - $rc2.left) -gt 800 -or $want -ne "WorkerW") { $target = $t; $name = $want; break }
    }
  }
  if ($target -ne [IntPtr]::Zero) { break }
}
if ($target -eq [IntPtr]::Zero) {
  # WPE 窗口是 Progman 的子窗口，顶层枚举不到 → 遍历 Progman 子级
  $progman = [IntPtr]::Zero
  foreach ($t in $tops) { if ([WP]::ClassOf($t) -eq "Progman") { $progman = $t; break } }
  foreach ($ch in [WP]::Children($progman)) {
    if ([WP]::ClassOf($ch) -eq "WPEDesktopDX11Window") { $target = $ch; $name = "WPEDesktopDX11Window(child)"; break }
  }
}
Write-Host "抓取目标：$name hwnd=$target"

$w = 0; $ht = 0
$print = [WP]::PrintShot($target, [ref]$w, [ref]$ht)
if ($print -ne $null) {
  [WP]::SaveBmp($print, $w, $ht, "E:\tools\ring-dock\target\wallpaper_print.bmp")
  Write-Host "PrintWindow 抓取成功 ${w}x${ht} → wallpaper_print.bmp"
} else {
  Write-Host "PrintWindow 失败"
}
$rc = New-Object WP+RECT
[WP]::GetWindowRect($target, [ref]$rc) | Out-Null
$sw = [int]($rc.right - $rc.left); $sh = [int]($rc.bottom - $rc.top)
# 屏幕合成抓取同一区域中部 800x600（对比用：应当含应用窗口）
$cx = [int](($rc.left + $rc.right) / 2) - 400; $cy = [int](($rc.top + $rc.bottom) / 2) - 300
$scr = [WP]::ScreenShot($cx, $cy, 800, 600)
[WP]::SaveBmp($scr, 800, 600, "E:\tools\ring-dock\target\screen_composed.bmp")
Write-Host "屏幕合成抓取 → screen_composed.bmp (对比：是否含应用窗口)"

# 数值对比：在 PrintWindow 结果里取中部 800x600 区域，与屏幕合成抓取逐点比
if ($print -ne $null -and $w -ge 800 -and $ht -ge 600) {
  $strideP = ((3 * $w + 3) -band -4)
  $strideS = ((3 * 800 + 3) -band -4)
  $ox = [int](($w - 800) / 2); $oy = [int](($ht - 600) / 2)
  $sum = 0L; $n = 0; $samples = ""
  foreach ($fy in @(0.2, 0.35, 0.5, 0.65, 0.8)) {
    foreach ($fx in @(0.2, 0.35, 0.5, 0.65, 0.8)) {
      $px = [int](800 * $fx); $py = [int](600 * $fy)
      $ip = ($oy + $py) * $strideP + ($ox + $px) * 3
      $is = $py * $strideS + $px * 3
      $d = [Math]::Abs($print[$ip] - $scr[$is]) + [Math]::Abs($print[$ip+1] - $scr[$is+1]) + [Math]::Abs($print[$ip+2] - $scr[$is+2])
      $sum += $d; $n++
      if ($n -le 5) { $samples += "[{0},{1},{2}]/[{3},{4},{5}] " -f $print[$ip+2], $print[$ip+1], $print[$ip], $scr[$is+2], $scr[$is+1], $scr[$is] }
    }
  }
  Write-Host ("[对比] PrintWindow vs 屏幕合成：平均像素差={0:N1} / 765（0=完全一致=拿到纯壁纸）" -f ($sum / $n))
  Write-Host ("[采样 RGB] PrintWindow/屏幕: $samples")
}

# 决定性验证：挂件可见时，PrintWindow 是否含挂件自身像素（含=反馈污染，不能当壁纸源）
$h = [WP]::FindAny("RingDockVisual")
if ($print -ne $null -and $h -ne [IntPtr]::Zero) {
  $wr = New-Object WP+RECT
  [WP]::GetWindowRect($h, [ref]$wr) | Out-Null
  $wcx = [int](($wr.left + $wr.right) / 2); $wcy = [int](($wr.top + $wr.bottom) / 2)
  $strideP = ((3 * $w + 3) -band -4)
  $strideS = ((3 * 800 + 3) -band -4)
  function SamplePair([int]$sx, [int]$sy) {
    $ip = ($sy - $rc.top) * $strideP + ($sx - $rc.left) * 3
    $is = ($sy - $cy) * $strideS + ($sx - $cx) * 3
    return ("Print=({0},{1},{2}) Screen=({3},{4},{5})" -f $print[$ip+2], $print[$ip+1], $print[$ip], $scr[$is+2], $scr[$is+1], $scr[$is])
  }
  Write-Host "[挂件像素污染测试] 挂件圆心=($wcx,$wcy)"
  Write-Host ("  中心(白玻璃区): " + (SamplePair $wcx $wcy))
  Write-Host ("  圆盘墨区(圆心-120): " + (SamplePair ($wcx - 120) $wcy))
  Write-Host ("  圆盘外(圆心-200): " + (SamplePair ($wcx - 200) $wcy))
} else {
  Write-Host "[挂件像素污染测试] 未找到挂件窗口或 PrintWindow 失败，跳过"
}
