# 色键透明探针：LWA_COLORKEY 在「桌面子窗口」下是否生效（键色像素=真透明、露壁纸）？
# 若生效 → 中心圆可以做到“真的透明”（壁纸透出），无需抓屏、无需逐像素 alpha
Add-Type -AssemblyName System.Drawing
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class CK {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  public delegate IntPtr WndProc(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct WNDCLASSW { public uint style; public WndProc lpfnWndProc; public int cbClsExtra, cbWndExtra; public IntPtr hInstance, hIcon, hCursor, hbrBackground; public string lpszMenuName, lpszClassName; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left, top, right, bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct PAINTSTRUCT { public IntPtr hdc; public bool fErase; public RECT rcPaint; public bool fRestore, fIncUpdate; public uint reserved0, reserved1, reserved2, reserved3, reserved4, reserved5; }
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern ushort RegisterClassW(ref WNDCLASSW c);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr CreateWindowExW(uint ex, string cls, string title, uint style, int x, int y, int w, int h, IntPtr parent, IntPtr menu, IntPtr inst, IntPtr param);
  [DllImport("user32.dll")] static extern IntPtr DefWindowProcW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetLayeredWindowAttributes(IntPtr h, uint key, byte alpha, uint flags);
  [DllImport("user32.dll")] public static extern int SetWindowLongW(IntPtr h, int i, int v);
  [DllImport("user32.dll")] public static extern int GetWindowLongW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern IntPtr SetParent(IntPtr c, IntPtr p);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT rc);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] static extern bool PeekMessageW(out MSG m, IntPtr h, uint min, uint max, uint remove);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr BeginPaint(IntPtr h, out PAINTSTRUCT ps);
  [DllImport("user32.dll")] static extern bool EndPaint(IntPtr h, ref PAINTSTRUCT ps);
  [DllImport("user32.dll")] static extern bool InvalidateRect(IntPtr h, IntPtr rc, bool erase);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateSolidBrush(uint c);
  [DllImport("user32.dll")] static extern bool FillRect(IntPtr hdc, ref RECT rc, IntPtr br);
  [DllImport("gdi32.dll")]  static extern bool Rectangle(IntPtr hdc, int l, int t, int r, int b);
  [DllImport("gdi32.dll")]  static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
  [DllImport("gdi32.dll")]  static extern bool DeleteObject(IntPtr o);
  [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [StructLayout(LayoutKind.Sequential)] public struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam, lParam; public uint time; public POINT pt; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int x, y; }

  public const uint KEY = 0x00FF00FF; // 品红 = 键色（应完全透明）
  public static int Clicks = 0;
  public static string LastClick = "";

  public static void Paint(IntPtr h) {
    RECT rc; GetWindowRect(h, out rc);
    IntPtr hdc = GetDC(h);
    RECT full = new RECT { left = 0, top = 0, right = rc.right - rc.left, bottom = rc.bottom - rc.top };
    IntPtr keyBrush = CreateSolidBrush(KEY);
    IntPtr darkBrush = CreateSolidBrush(0x0000FF00); // 纯绿（判别：看到绿=绘制生效）
    FillRect(hdc, ref full, keyBrush);            // 整窗 = 键色（应透明）
    RECT half = new RECT { left = 0, top = (rc.bottom - rc.top) / 2, right = rc.right - rc.left, bottom = rc.bottom - rc.top };
    FillRect(hdc, ref half, darkBrush);           // 下半 = 不透明墨色（应可见+可点）
    DeleteObject(keyBrush); DeleteObject(darkBrush);
    ReleaseDC(h, hdc);
  }

  static IntPtr WndProcImpl(IntPtr h, uint m, IntPtr w, IntPtr l) {
    if (m == 0x000F) { // WM_PAINT
      PAINTSTRUCT ps; BeginPaint(h, out ps); Paint(h); EndPaint(h, ref ps);
      return IntPtr.Zero;
    }
    if (m == 0x0201) {
      Clicks++;
      long v = l.ToInt64();
      LastClick = ((int)(v & 0xFFFF)) + "," + ((int)((v >> 16) & 0xFFFF));
      return IntPtr.Zero;
    }
    return DefWindowProcW(h, m, w, l);
  }

  public static IntPtr Create(int x, int y, int w, int h, uint exstyle, string cls) {
    WNDCLASSW wc = new WNDCLASSW();
    wc.lpfnWndProc = WndProcImpl;
    wc.hInstance = GetModuleHandleW(null);
    wc.lpszClassName = cls;
    RegisterClassW(ref wc);
    return CreateWindowExW(exstyle, cls, "ck-probe", 0x80000000, x, y, w, h, IntPtr.Zero, IntPtr.Zero, GetModuleHandleW(null), IntPtr.Zero);
  }
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern IntPtr GetModuleHandleW(string n);

  public static void Pump(int ms) {
    DateTime end = DateTime.Now.AddMilliseconds(ms);
    MSG m;
    while (DateTime.Now < end) {
      while (PeekMessageW(out m, IntPtr.Zero, 0, 0, 1)) { TranslateMessage(ref m); DispatchMessageW(ref m); }
      System.Threading.Thread.Sleep(5);
    }
  }
  public static void RealClick(int x, int y) {
    Clicks = 0; LastClick = "";
    SetCursorPos(x, y); System.Threading.Thread.Sleep(60);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero); mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
    Pump(300);
  }
  public static string ClassOf(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }
  public static IntPtr Find(string cls) {
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
  public static IntPtr[] Children(IntPtr p) {
    System.Collections.Generic.List<IntPtr> list = new System.Collections.Generic.List<IntPtr>();
    EnumProc cb = (h, l) => { list.Add(h); return true; };
    EnumChildWindows(p, cb, IntPtr.Zero);
    return list.ToArray();
  }
  public static string Embed(IntPtr h) {
    // ★ 只找桌面的宿主：Progman 的子级 SysListView32（避免误拄到资源管理器窗口的列表）
    IntPtr progman = Find("Progman");
    IntPtr host = IntPtr.Zero;
    foreach (IntPtr ch in Children(progman)) {
      string c = ClassOf(ch);
      if (c == "SysListView32" || c == "SHELLDLL_DefView") { host = ch; break; }
    }
    if (host == IntPtr.Zero) host = progman;
    for (IntPtr p = host; p != IntPtr.Zero; p = GetParent(p)) {
      int ps = GetWindowLongW(p, -16);
      SetWindowLongW(p, -16, ps | 0x02000000); // WS_CLIPCHILDREN
    }
    int style = GetWindowLongW(h, -16);
    SetWindowLongW(h, -16, (style & ~unchecked((int)0x80000000)) | 0x40000000); // WS_POPUP→WS_CHILD
    IntPtr old = SetParent(h, host);
    // 子窗口坐标=相对父客户区：把窗口归位到屏幕 (200,120)
    IntPtr hdc = GetDC(host);
    POINT org = new POINT();
    ClientToScreen(host, ref org);
    ReleaseDC(host, hdc);
    SetWindowPos(h, IntPtr.Zero, 200 - org.x, 120 - org.y, 0, 0, 0x0001 | 0x0004); // NOMOVE→MOVE, NOSIZE
    return "host=" + ClassOf(host) + " parent_org=" + org.x + "," + org.y + " getparent_now=" + GetParent(h);
  }
  [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  public static string WinRect(IntPtr h) {
    RECT rc; GetWindowRect(h, out rc);
    return rc.left + "," + rc.top + "," + rc.right + "," + rc.bottom;
  }
}
"@
Add-Type -TypeDefinition $src
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class KD2 {
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr ex);
  public static void WinD() {
    keybd_event(0x5B, 0, 0, IntPtr.Zero); keybd_event(0x44, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 2, IntPtr.Zero); keybd_event(0x5B, 0, 2, IntPtr.Zero);
  }
}
"@

function Shot([int]$x, [int]$y, [int]$w, [int]$h, [string]$out) {
  $b = New-Object System.Drawing.Bitmap($w, $h)
  $g = [System.Drawing.Graphics]::FromImage($b)
  $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size($w, $h)))
  $b.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $b.Dispose()
}

$S = 220
# WS_EX_LAYERED(0x80000) | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
$h = [CK]::Create(200, 120, $S, $S, 0x00080000 -bor 0x00000080 -bor 0x08000000, "ColorKeyProbe")
[CK]::SetLayeredWindowAttributes($h, [CK]::KEY, 255, 0x00000001) | Out-Null  # LWA_COLORKEY
[CK]::ShowWindow($h, 8) | Out-Null
[CK]::Paint($h)
[CK]::Pump(300)
[KD2]::WinD(); Start-Sleep -Seconds 1
[CK]::ShowWindow($h, 8) | Out-Null
[CK]::Paint($h)
[CK]::Pump(300)
$wr = [CK]::WinRect($h) -split ","
Write-Host ("[1] 顶层+色键 窗口矩形=" + [CK]::WinRect($h))
Shot ([int]$wr[0] - 80) ([int]$wr[1] - 80) ($S + 160) ($S + 160) "E:\tools\ring-dock\target\ck_toplevel.png"

# 采样：上半（键色区，应=壁纸）与下半（墨色区，应=(23,25,33)）+ 窗外壁纸参考
Add-Type -AssemblyName System.Drawing
$b = [System.Drawing.Bitmap]::FromFile("E:\tools\ring-dock\target\ck_toplevel.png")
$pTop = $b.GetPixel(190, 120); $pBot = $b.GetPixel(190, 260); $pOut = $b.GetPixel(40, 40)
Write-Host ("[1] 上半(键色区) =({0},{1},{2}) 期望=壁纸色  下半(纯绿区)=({3},{4},{5}) 期望=(0,255,0)  窗外参考=({6},{7},{8})" -f $pTop.R, $pTop.G, $pTop.B, $pBot.R, $pBot.G, $pBot.B, $pOut.R, $pOut.G, $pOut.B)
$b.Dispose()

# 命中：上半（透明区）点击应穿透；下半（实色区）点击应命中
[CK]::RealClick(310, 160)
Write-Host ("[1] 透明区点击 Clicks={0} (期望 0=穿透)" -f [CK]::Clicks)
[CK]::RealClick(310, 300)
Write-Host ("[1] 实色区点击 Clicks={0} (期望 1=命中)" -f [CK]::Clicks)

# ============ 嵌入桌面树 ============
Write-Host ("[2] " + [CK]::Embed($h))
[CK]::ShowWindow($h, 8) | Out-Null
[CK]::Paint($h)
[CK]::Pump(300)
$wr = [CK]::WinRect($h) -split ","
Write-Host ("[2] 嵌入后矩形=" + [CK]::WinRect($h))
Shot ([int]$wr[0] - 80) ([int]$wr[1] - 80) ($S + 160) ($S + 160) "E:\tools\ring-dock\target\ck_embedded.png"
$b = [System.Drawing.Bitmap]::FromFile("E:\tools\ring-dock\target\ck_embedded.png")
$pTop = $b.GetPixel(190, 120); $pBot = $b.GetPixel(190, 260); $pOut = $b.GetPixel(40, 40)
Write-Host ("[2] 上半(键色区) =({0},{1},{2}) 期望=壁纸色  下半(纯绿区)=({3},{4},{5}) 期望=(0,255,0)  窗外参考=({6},{7},{8})" -f $pTop.R, $pTop.G, $pTop.B, $pBot.R, $pBot.G, $pBot.B, $pOut.R, $pOut.G, $pOut.B)
$b.Dispose()
[CK]::RealClick([int]([int]$wr[0] + 110), [int]([int]$wr[1] + 40))
Write-Host ("[2] 透明区点击 Clicks={0} (期望 0=穿透)" -f [CK]::Clicks)
[CK]::RealClick([int]([int]$wr[0] + 110), [int]([int]$wr[1] + 180))
Write-Host ("[2] 实色区点击 Clicks={0} (期望 1=命中)" -f [CK]::Clicks)
[KD2]::WinD(); Start-Sleep -Milliseconds 500
Write-Host "探针完成（桌面已还原）"
