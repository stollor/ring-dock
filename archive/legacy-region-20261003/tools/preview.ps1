# 视觉预览：真实屏幕截图（收起态 / 展开态）+ 盘缘抗锯齿量化
# 会短暂 Win+D 显示桌面（被其他窗口遮挡时），结束自动还原。
Add-Type -AssemblyName System.Drawing
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class PV {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left, top, right, bottom; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr ex);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowLongW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT rc);
  [DllImport("user32.dll")] public static extern bool SystemParametersInfoW(uint a, uint b, out RECT rc, uint f);

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
    keybd_event(0x5B, 0, 0, IntPtr.Zero); keybd_event(0x44, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 2, IntPtr.Zero); keybd_event(0x5B, 0, 2, IntPtr.Zero);
  }
  public static void RealClick(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(80);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(200);
  }
}
"@
Add-Type -TypeDefinition $src

function Shot([int]$cx, [int]$cy, [int]$half, [string]$out) {
  $size = 2 * $half
  $b = New-Object System.Drawing.Bitmap($size, $size)
  $g = [System.Drawing.Graphics]::FromImage($b)
  $g.CopyFromScreen(($cx - $half), ($cy - $half), 0, 0, (New-Object System.Drawing.Size($size, $size)))
  $b.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $b.Dispose()
}

# 45° 斜边抗锯齿度量：从圆心沿 225° 方向扫描盘缘，统计过渡像素数
function EdgeAA([string]$path, [int]$cx, [int]$cy, [double]$r) {
  $b = [System.Drawing.Bitmap]::FromFile($path)
  $vals = @()
  for ($i = -8; $i -le 8; $i++) {
    $ang = 225 * [Math]::PI / 180
    $x = [int][Math]::Round($cx + ($r + $i) * [Math]::Cos($ang))
    $y = [int][Math]::Round($cy + ($r + $i) * [Math]::Sin($ang))
    $p = $b.GetPixel($x, $y)
    $vals += , @($p.R, $p.G, $p.B)
  }
  $b.Dispose()
  $fg = $vals[0]; $bg = $vals[$vals.Count - 1]
  $mid = 0; $s = ""
  foreach ($v in $vals) {
    $dF = [Math]::Abs($v[0] - $fg[0]) + [Math]::Abs($v[1] - $fg[1]) + [Math]::Abs($v[2] - $fg[2])
    $dB = [Math]::Abs($v[0] - $bg[0]) + [Math]::Abs($v[1] - $bg[1]) + [Math]::Abs($v[2] - $bg[2])
    if ($dF -gt 6 -and $dB -gt 6) { $mid++ }
    $s += "({0},{1},{2}) " -f $v[0], $v[1], $v[2]
  }
  return "过渡像素=$mid / 15  序列=$s"
}

$p = Start-Process "E:\tools\ring-dock\target\release\ring-dock.exe" -PassThru
Start-Sleep -Seconds 2
$h = [PV]::FindAny("RingDockVisual")
$wa = [PV]::WorkArea(); $cx = [int]($wa[0] + $wa[2] / 2); $cy = [int]($wa[1] + $wa[3] / 2)
Write-Host "窗口=$h  圆心=($cx,$cy)"

$needWind = [PV]::Covered($cx - 260, $cy - 260, 520, 520)
if ($needWind) { [PV]::WinD(); Start-Sleep -Seconds 1; Write-Host "[环境] 已显示桌面（结束还原）" }

# 1) 收起态
Shot $cx $cy 420 "E:\tools\ring-dock\target\preview_ring.png"
Write-Host ("[盘缘 AA] " + (EdgeAA "E:\tools\ring-dock\target\preview_ring.png" 420 420 144))

# 2) 展开面板（点右上象限弧段）
$ax = [int]($cx + 117 * [Math]::Cos(-45 * [Math]::PI / 180))
$ay = [int]($cy + 117 * [Math]::Sin(-45 * [Math]::PI / 180))
[PV]::RealClick($ax, $ay)
Start-Sleep -Milliseconds 800
Shot $cx $cy 420 "E:\tools\ring-dock\target\preview_panel.png"
Write-Host "[截图] preview_ring.png / preview_panel.png"

# 收起面板 + 还原环境
[PV]::RealClick($cx, $cy)
Start-Sleep -Milliseconds 500
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
if ($needWind) { [PV]::WinD(); Start-Sleep -Milliseconds 500 }
Write-Host "完成（桌面已还原）"
