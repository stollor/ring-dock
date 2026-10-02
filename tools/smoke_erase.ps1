# 残影回归：桌面子窗口 + SetWindowRgn 缩小后暴露区必须交还桌面重绘
# 观测：面板区域截图像素对比（收起/切换后应还原为展开前的桌面像素）
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public delegate bool EnumProc(IntPtr h, IntPtr l);
public class E {
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SystemParametersInfoW(uint a, uint b, out RECT rc, uint f);
  [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateCompatibleDC(IntPtr dc);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateCompatibleBitmap(IntPtr dc, int w, int h);
  [DllImport("gdi32.dll")]  static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
  [DllImport("gdi32.dll")]  static extern bool BitBlt(IntPtr dst, int x, int y, int w, int h, IntPtr src, int sx, int sy, uint rop);
  [DllImport("gdi32.dll")]  static extern int GetDIBits(IntPtr dc, IntPtr bmp, uint start, uint lines, byte[] buf, ref BITMAPINFO bi, uint usage);
  [DllImport("gdi32.dll")]  static extern bool DeleteObject(IntPtr o);
  [DllImport("gdi32.dll")]  static extern bool DeleteDC(IntPtr dc);
  public struct RECT { public int left, top, right, bottom; }
  [StructLayout(LayoutKind.Sequential)]
  public struct BITMAPINFOHEADER { public uint biSize; public int biWidth, biHeight; public ushort biPlanes, biBitCount; public uint biCompression, biSizeImage; public int biXPelsPerMeter, biYPelsPerMeter; public uint biClrUsed, biClrImportant; }
  [StructLayout(LayoutKind.Sequential)]
  public struct BITMAPINFO { public BITMAPINFOHEADER bmiHeader; public uint bmiColors; }

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
  public static int[] WorkArea() {
    RECT rc;
    SystemParametersInfoW(0x0030, 0, out rc, 0);
    return new int[] { rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top };
  }
  // 截取屏幕区域 → 24bpp 像素流（与项目 sys.rs 的 capture_screen 同一手法）
  public static byte[] Snap(int x, int y, int w, int h) {
    IntPtr screen = GetDC(IntPtr.Zero);
    IntPtr memdc = CreateCompatibleDC(screen);
    IntPtr bmp = CreateCompatibleBitmap(screen, w, h);
    IntPtr old = SelectObject(memdc, bmp);
    BitBlt(memdc, 0, 0, w, h, screen, x, y, 0x00CC0020); // SRCCOPY
    var bi = new BITMAPINFO();
    bi.bmiHeader.biSize = (uint)Marshal.SizeOf(typeof(BITMAPINFOHEADER));
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h; // 自上而下
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 24;
    byte[] buf = new byte[w * h * 3 + w * 4]; // 24bpp 每行 4 字节对齐
    GetDIBits(memdc, bmp, 0, (uint)h, buf, ref bi, 0);
    SelectObject(memdc, old);
    DeleteObject(bmp);
    DeleteDC(memdc);
    ReleaseDC(IntPtr.Zero, screen);
    return buf;
  }
  // 平均绝对差（每通道 0-255）
  public static double Diff(byte[] a, byte[] b) {
    int n = Math.Min(a.Length, b.Length);
    long sum = 0;
    for (int i = 0; i < n; i++) sum += Math.Abs(a[i] - b[i]);
    return (double)sum / n;
  }
  public static void Click(IntPtr h, int x, int y) {
    IntPtr lp = (IntPtr)(((y & 0xFFFF) << 16) | (x & 0xFFFF));
    SendMessageW(h, 0x0201, (IntPtr)0x0001, lp);
    SendMessageW(h, 0x0202, (IntPtr)0x0000, lp);
  }
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr ex);
  // Win+D 显示桌面 / 再按还原（测试期间避免其他窗口遮挡观测区）
  public static void WinD() {
    keybd_event(0x5B, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 2, IntPtr.Zero);
    keybd_event(0x5B, 0, 2, IntPtr.Zero);
  }
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] static extern int GetWindowLongW(IntPtr h, int i);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT rc);
  // 观测区是否被可见应用窗口遮挡（忽略最小化窗口、工具窗、桌面壳；挂件是子窗口不会被枚举）
  public static bool Covered(int x, int y, int w, int h) {
    bool covered = false;
    RECT target = new RECT { left = x, top = y, right = x + w, bottom = y + h };
    EnumProc cb = null;
    cb = (hwnd, l) => {
      if (!IsWindowVisible(hwnd) || IsIconic(hwnd)) return true;
      long ex = GetWindowLongW(hwnd, -20) & 0xFFFFFFFFL;
      if ((ex & 0x80) != 0) return true; // WS_EX_TOOLWINDOW
      string c = ClassOf(hwnd);
      if (c == "Progman" || c == "WorkerW" || c == "Shell_TrayWnd" || c == "Windows.UI.Core.CoreWindow") return true;
      RECT rc; GetWindowRect(hwnd, out rc);
      if (rc.left < target.right && rc.right > target.left && rc.top < target.bottom && rc.bottom > target.top) {
        covered = true; return false;
      }
      return true;
    };
    EnumWindows(cb, IntPtr.Zero);
    return covered;
  }
  [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int a, int b, int c, int d);
  [DllImport("gdi32.dll")] static extern int GetRgnBox(IntPtr rgn, out RECT rc);
  [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h, IntPtr rgn);
  // 命中区域包围盒（诊断用：确认点击是否生效）
  public static string RgnBox(IntPtr h) {
    IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
    GetWindowRgn(h, rgn);
    RECT rc; GetRgnBox(rgn, out rc);
    return string.Format("{0},{1},{2},{3}", rc.left, rc.top, rc.right, rc.bottom);
  }
  // 24bpp 像素流 → BMP 文件（人工拼文件头，便于可视化诊断）
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

function Fail($msg) {
  Write-Host "[FAIL] $msg"
  # 失败前主动收起面板（点中心），避免残影污染测试环境
  if ($script:h -and $script:h -ne [IntPtr]::Zero) {
    [E]::Click($script:h, $script:cx, $script:cy); Start-Sleep -Milliseconds 500
  }
  if ($script:proc -and -not $script:proc.HasExited) { Stop-Process -Id $script:proc.Id -Force -ErrorAction SilentlyContinue }
  if ($script:showDesktop) { [E]::WinD(); Start-Sleep -Milliseconds 500 }  # 还原被最小化的窗口
  exit 1
}

$script:proc = Start-Process -FilePath "E:\tools\ring-dock\target\release\ring-dock.exe" -PassThru
Start-Sleep -Seconds 2
$script:h = [E]::FindAny("RingDockVisual")
if ($script:h -eq [IntPtr]::Zero) { Fail "找不到主窗口" }
$h = $script:h

# 显示桌面：仅在观测区被其他应用窗口遮挡时才 Win+D（Win+D 是开关式，必须按需触发）
$script:showDesktop = $false
if ([E]::Covered($rx, $ry, $rw, $rh)) {
  [E]::WinD(); $script:showDesktop = $true; Start-Sleep -Seconds 1
  Write-Host "[环境] 观测区被遮挡，已 Win+D 显示桌面（结束时自动还原）"
  if ([E]::Covered($rx, $ry, $rw, $rh)) { Fail "观测区仍被遮挡，无法测试" }
} else {
  Write-Host "[环境] 观测区无遮挡"
}

# 面板 0（右上，圆环外展开）内的纯面板子区域
$wa = [E]::WorkArea()
$script:cx = [int]($wa[2] / 2); $script:cy = [int]($wa[3] / 2)
$cx = $script:cx; $cy = $script:cy
$rx = $cx + 280; $ry = $cy - 316; $rw = 170; $rh = 150
$arc0x = [int]($cx + 117 * [Math]::Cos(-45 * [Math]::PI / 180)); $arc0y = [int]($cy + 117 * [Math]::Sin(-45 * [Math]::PI / 180))
$arc1x = [int]($cx + 117 * [Math]::Cos( 45 * [Math]::PI / 180)); $arc1y = [int]($cy + 117 * [Math]::Sin( 45 * [Math]::PI / 180))

Start-Sleep -Milliseconds 800
$before = [E]::Snap($rx, $ry, $rw, $rh)
$beforeSum = ($before | Measure-Object -Sum).Sum
Write-Host "[基线截图] 已取（面板区域 ${rw}x${rh}）字节=$($before.Length) 像素和=$beforeSum 基线Rgn=$([E]::RgnBox($h))"

# 1. 展开面板 0：区域应明显变化（证明面板真的覆盖了这里，测试有效）
[E]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 800
Write-Host "[诊断] 点弧0后 Rgn=$([E]::RgnBox($h))（基线 Rgn=1149,565,1410,826，变大=点击生效）"
$open = [E]::Snap($rx, $ry, $rw, $rh)
$openSum = ($open | Measure-Object -Sum).Sum
$d = [E]::Diff($before, $open)
Write-Host "[展开时差异] $d  展开后像素和=$openSum"
if ($d -lt 5) {
  [E]::SaveBmp($before, $rw, $rh, "E:\tools\ring-dock\target\erase_before.bmp")
  [E]::SaveBmp($open,   $rw, $rh, "E:\tools\ring-dock\target\erase_open.bmp")
  Fail "展开面板时区域无变化（测试无效，面板没盖住观测区？）"
}
Write-Host "[PASS] 展开面板覆盖观测区（测试有效）"

# 2. 收起面板 0：面板内容必须消失（残影 = 收起后像素 ≈ 展开时像素）
#    注：壁纸是动态的 + 可能有历史残影，二者在 before/open/after 中恒定，
#    因此判定用 Diff(after, open)，不受壁纸运动与历史残影影响。
[E]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 1000
$after = [E]::Snap($rx, $ry, $rw, $rh)
$dGone = [E]::Diff($after, $open)
$dBack = [E]::Diff($before, $after)
Write-Host "[收起后] vs展开时差异=$dGone（面板应消失）  vs基线差异=$dBack（壁纸动态，仅供参考）"
if ($dGone -lt 5) {
  [E]::SaveBmp($open,  $rw, $rh, "E:\tools\ring-dock\target\erase_open.bmp")
  [E]::SaveBmp($after, $rw, $rh, "E:\tools\ring-dock\target\erase_after.bmp")
  Write-Host "[诊断] 图已导出到 target\erase_*.bmp"
  Fail "收起后面板残影未清除（与展开时差异 $dGone < 5，面板像素残留）"
}
Write-Host "[PASS] 收起后面板内容消失（差异 $dGone > 5）"

# 3. 切换场景：展开 0 → 切到 1：面板 0 区域的面板内容同样必须消失
[E]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 800
$open2 = [E]::Snap($rx, $ry, $rw, $rh)
[E]::Click($h, $arc1x, $arc1y); Start-Sleep -Milliseconds 1000
$after = [E]::Snap($rx, $ry, $rw, $rh)
$dGone = [E]::Diff($after, $open2)
Write-Host "[切换后] 旧面板区 vs展开时差异=$dGone（旧面板应消失）Rgn=$([E]::RgnBox($h))"
if ($dGone -lt 5) {
  [E]::SaveBmp($open2, $rw, $rh, "E:\tools\ring-dock\target\erase_open.bmp")
  [E]::SaveBmp($after, $rw, $rh, "E:\tools\ring-dock\target\erase_after.bmp")
  Fail "切换面板后旧面板残影未清除（差异 $dGone < 5）"
}
Write-Host "[PASS] 切换后旧面板内容消失（差异 $dGone > 5）"

Stop-Process -Id $script:proc.Id -Force -ErrorAction SilentlyContinue
if ($script:showDesktop) { [E]::WinD(); Start-Sleep -Milliseconds 500 }  # 还原被最小化的窗口
Write-Host "[OK] 残影回归通过"
