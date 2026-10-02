# 命中验证：真实鼠标（系统 hit-test）点击"弧段区域内但不是弧线像素"的空白处
# 修复前：色键透明像素不接收鼠标 → 穿透无效；现在：命中垫按 Rgn 命中 → 应展开面板
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public delegate bool EnumProc(IntPtr h, IntPtr l);
public class H {
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, int d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SystemParametersInfoW(uint a, uint b, out RECT rc, uint f);
  [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h, IntPtr rgn);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateRectRgn(int a, int b, int c, int d);
  [DllImport("gdi32.dll")]  static extern int GetRgnBox(IntPtr rgn, out RECT rc);
  public struct RECT { public int left, top, right, bottom; }

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
    RECT rc; SystemParametersInfoW(0x0030, 0, out rc, 0);
    return new int[] { rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top };
  }
  public static string RgnBox(IntPtr h) {
    IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
    GetWindowRgn(h, rgn);
    RECT rc; GetRgnBox(rgn, out rc);
    return string.Format("{0},{1},{2},{3}", rc.left, rc.top, rc.right, rc.bottom);
  }
  // 真实鼠标点击（走系统 hit-test）
  public static void RealClick(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(80);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero); // LEFTDOWN
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero); // LEFTUP
  }
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] static extern int GetWindowLongW(IntPtr h, int i);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT rc);
  public static bool Covered(int x, int y, int w, int h) {
    bool covered = false;
    RECT target = new RECT { left = x, top = y, right = x + w, bottom = y + h };
    EnumProc cb = null;
    cb = (hwnd, l) => {
      if (!IsWindowVisible(hwnd) || IsIconic(hwnd)) return true;
      long ex = GetWindowLongW(hwnd, -20) & 0xFFFFFFFFL;
      if ((ex & 0x80) != 0) return true;
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
  public static void WinD() {
    keybd_event(0x5B, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 2, IntPtr.Zero);
    keybd_event(0x5B, 0, 2, IntPtr.Zero);
  }
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr ex);
}
"@
Add-Type -TypeDefinition $src

$p = Start-Process "E:\tools\ring-dock\target\release\ring-dock.exe" -PassThru
Start-Sleep -Seconds 2
$h = [H]::FindAny("RingDockVisual")
$hit = [H]::FindAny("RingDockHit")
Write-Host "视觉层=$h  命中垫=$hit"
$base = [H]::RgnBox($h)
Write-Host "[基线 Rgn] $base"

$wa = [H]::WorkArea(); $cx = [int]($wa[2]/2); $cy = [int]($wa[3]/2)
# 遮挡则显示桌面（点击点被其他窗口吃掉会误判命中失败）
$needWind = [H]::Covered($cx - 140, $cy - 140, 280, 280)
if ($needWind) { [H]::WinD(); Start-Sleep -Seconds 1; Write-Host "[环境] 已显示桌面" }
# 点"象限0 弧段范围内、但离弧线 13px 的空白处"（旧版色键下=穿透无效）
$blankX = $cx + [int](130 * [Math]::Cos(-45*[Math]::PI/180))
$blankY = $cy + [int](130 * [Math]::Sin(-45*[Math]::PI/180))
Write-Host "真实点击空白处（环带内、非弧线像素）: ($blankX,$blankY)"
[H]::RealClick($blankX, $blankY)
Start-Sleep -Milliseconds 600
$after = [H]::RgnBox($h)
Write-Host "[点击后 Rgn] $after"
if ($after -eq $base) {
  Write-Host "[FAIL] 点击无效（空白处未命中）"
  if ($needWind) { [H]::WinD() }
  Stop-Process -Id $p.Id -Force; exit 1
}
Write-Host "[PASS] 空白处真实鼠标命中成功（面板已展开）"
# 再点一次收起
[H]::RealClick($blankX, $blankY)
Start-Sleep -Milliseconds 600
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
if ($needWind) { [H]::WinD(); Start-Sleep -Milliseconds 500 }
Write-Host "[OK] 命中验证通过"
