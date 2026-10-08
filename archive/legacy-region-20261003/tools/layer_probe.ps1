# 分层窗口（逐像素 alpha）可行性探针 —— 回答 4 个问题：
#  Q1 逐像素 alpha 圆的边缘在屏幕上是否平滑（AA 生效）？
#  Q2 alpha=255 实心区真实鼠标可命中吗？alpha=0 区是否穿透？
#  Q3 SetParent 进桌面树后，以上两点是否仍然成立？
#  Q4 SetWindowRgn 会不会裁掉分层窗口的显示（决定回归工具能否继续用 Rgn 观测）？
param([int]$Size = 220)
Add-Type -AssemblyName System.Drawing

$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class LP {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  public delegate IntPtr WndProc(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct WNDCLASSW { public uint style; public WndProc lpfnWndProc; public int cbClsExtra; public int cbWndExtra; public IntPtr hInstance; public IntPtr hIcon; public IntPtr hCursor; public IntPtr hbrBackground; public string lpszMenuName; public string lpszClassName; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int x, y; }
  [StructLayout(LayoutKind.Sequential)] public struct SIZE { public int cx, cy; }
  [StructLayout(LayoutKind.Sequential)] public struct BLENDFUNCTION { public byte BlendOp, BlendFlags, SourceConstantAlpha, AlphaFormat; }
  [StructLayout(LayoutKind.Sequential)] public struct BITMAPINFOHEADER { public uint biSize; public int biWidth, biHeight; public ushort biPlanes, biBitCount; public uint biCompression, biSizeImage; public int biXPelsPerMeter, biYPelsPerMeter; public uint biClrUsed, biClrImportant; }
  [StructLayout(LayoutKind.Sequential)] public struct BITMAPINFO { public BITMAPINFOHEADER bmiHeader; public uint bmiColors; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left, top, right, bottom; }

  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern ushort RegisterClassW(ref WNDCLASSW c);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr CreateWindowExW(uint ex, string cls, string title, uint style, int x, int y, int w, int h, IntPtr parent, IntPtr menu, IntPtr inst, IntPtr param);
  [DllImport("user32.dll")] static extern IntPtr DefWindowProcW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll", SetLastError = true)] public static extern bool UpdateLayeredWindow(IntPtr hwnd, IntPtr hdcDst, ref POINT pptDst, ref SIZE psize, IntPtr hdcSrc, ref POINT pptSrc, uint crKey, ref BLENDFUNCTION pblend, uint dwFlags);
  [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateCompatibleDC(IntPtr dc);
  [DllImport("gdi32.dll")]  static extern bool DeleteDC(IntPtr dc);
  [DllImport("gdi32.dll")]  static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
  [DllImport("gdi32.dll")]  static extern bool DeleteObject(IntPtr o);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateDIBSection(IntPtr hdc, ref BITMAPINFO bmi, uint usage, out IntPtr bits, IntPtr hsec, uint off);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr SetParent(IntPtr c, IntPtr p);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern int SetWindowLongW(IntPtr h, int i, int v);
  [DllImport("user32.dll")] public static extern int GetWindowLongW(IntPtr h, int i);
  [DllImport("gdi32.dll")]  public static extern IntPtr CreateEllipticRgn(int a, int b, int c, int d);
  [DllImport("user32.dll")] public static extern int SetWindowRgn(IntPtr h, IntPtr rgn, bool redraw);
  [DllImport("user32.dll")] public static extern int GetWindowRgn(IntPtr h, IntPtr rgn);
  [DllImport("gdi32.dll")]  public static extern IntPtr CreateRectRgn(int a, int b, int c, int d);
  [DllImport("gdi32.dll")]  public static extern int GetRgnBox(IntPtr rgn, out RECT rc);
  [DllImport("user32.dll")] static extern bool PeekMessageW(out MSG m, IntPtr h, uint min, uint max, uint remove);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref MSG m);
  [StructLayout(LayoutKind.Sequential)] public struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam, lParam; public uint time; public POINT pt; }

  public static int Clicks = 0;
  public static string LastClick = "";

  static IntPtr WndProcImpl(IntPtr h, uint m, IntPtr w, IntPtr l) {
    if (m == 0x0201) { // WM_LBUTTONDOWN
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
    wc.hInstance = GetModule();
    wc.lpszClassName = cls;
    RegisterClassW(ref wc);
    IntPtr hwnd = CreateWindowExW(exstyle, cls, "layer-probe", 0x80000000 /*WS_POPUP*/, x, y, w, h, IntPtr.Zero, IntPtr.Zero, GetModule(), IntPtr.Zero);
    return hwnd;
  }
  static IntPtr GetModule() {
    return GetModuleHandleW(null);
  }
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern IntPtr GetModuleHandleW(string n);

  public static string PresentErr = "";

  // 逐像素 alpha 呈现（premultiplied BGRA）；x/y = 窗口目标位置（UpdateLayeredWindow 会重设窗口位置）
  public static bool Present(IntPtr hwnd, byte[] bgra, int w, int h, int x, int y) {
    PresentErr = "";
    IntPtr hdcScreen = GetDC(IntPtr.Zero);
    IntPtr memdc = CreateCompatibleDC(hdcScreen);
    BITMAPINFO bmi = new BITMAPINFO();
    bmi.bmiHeader.biSize = 40;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    IntPtr bits;
    IntPtr dib = CreateDIBSection(memdc, ref bmi, 0, out bits, IntPtr.Zero, 0);
    if (dib == IntPtr.Zero) { PresentErr = "CreateDIBSection failed err=" + Marshal.GetLastWin32Error(); return false; }
    Marshal.Copy(bgra, 0, bits, w * h * 4);
    IntPtr old = SelectObject(memdc, dib);
    POINT ptDst = new POINT { x = x, y = y }, ptSrc = new POINT();
    SIZE sz = new SIZE { cx = w, cy = h };
    BLENDFUNCTION bf = new BLENDFUNCTION { BlendOp = 0, BlendFlags = 0, SourceConstantAlpha = 255, AlphaFormat = 1 };
    bool ok = UpdateLayeredWindow(hwnd, hdcScreen, ref ptDst, ref sz, memdc, ref ptSrc, 0, ref bf, 2 /*ULW_ALPHA*/);
    if (!ok) PresentErr = "UpdateLayeredWindow failed err=" + Marshal.GetLastWin32Error();
    SelectObject(memdc, old);
    DeleteObject(dib);
    DeleteDC(memdc);
    ReleaseDC(IntPtr.Zero, hdcScreen);
    return ok;
  }

  public static void Pump(int ms) {
    DateTime end = DateTime.Now.AddMilliseconds(ms);
    MSG m;
    while (DateTime.Now < end) {
      while (PeekMessageW(out m, IntPtr.Zero, 0, 0, 1)) {
        TranslateMessage(ref m);
        DispatchMessageW(ref m);
      }
      System.Threading.Thread.Sleep(5);
    }
  }

  public static void RealClick(int x, int y) {
    Clicks = 0; LastClick = "";
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(60);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
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
  public static string EmbedEx(IntPtr h, bool convertChild) {
    IntPtr host = Find("SysListView32");
    if (host == IntPtr.Zero) host = Find("SHELLDLL_DefView");
    if (host == IntPtr.Zero) host = Find("Progman");
    // ★ 与 deskpin 一致：父链加 WS_CLIPCHILDREN，否则父窗口重绘会把子窗口内容擦掉
    //   （上一轮探针漏了这步，可能导致“分层子窗口不可见”的假结论）
    for (IntPtr p = host; p != IntPtr.Zero; p = GetParent(p)) {
      int ps = GetWindowLongW(p, -16);
      SetWindowLongW(p, -16, ps | 0x02000000); // WS_CLIPCHILDREN
    }
    if (convertChild) {
      int style = GetWindowLongW(h, -16);
      SetWindowLongW(h, -16, (style & ~unchecked((int)0x80000000)) | 0x40000000); // WS_POPUP→WS_CHILD
    }
    IntPtr old = SetParent(h, host);
    IntPtr now = GetParent(h);
    return "host=" + host + "(" + ClassOf(host) + ") setparent_ret=" + old + " getparent_now=" + now;
  }
  public static void SetRgnEllipse(IntPtr h, int l, int t, int r, int b) {
    IntPtr rgn = CreateEllipticRgn(l, t, r, b);
    SetWindowRgn(h, rgn, true);
  }
  public static void ClearRgn(IntPtr h) {
    SetWindowRgn(h, IntPtr.Zero, true);
  }
  public static string RgnBox(IntPtr h) {
    IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
    int rc = GetWindowRgn(h, rgn);
    RECT box; GetRgnBox(rgn, out box);
    return "ret=" + rc + " box=" + box.left + "," + box.top + "," + box.right + "," + box.bottom;
  }
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT rc);
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
public class KD {
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr ex);
  public static void WinD() {
    keybd_event(0x5B, 0, 0, IntPtr.Zero); keybd_event(0x44, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 2, IntPtr.Zero); keybd_event(0x5B, 0, 2, IntPtr.Zero);
  }
}
"@

# ---- 逐像素 alpha 的圆：数学生成 AA 覆盖率（premultiplied BGRA）----
function MakeCircleBytes([int]$w, [int]$h, [double]$cx, [double]$cy, [double]$rad, [byte]$CR, [byte]$CG, [byte]$CB) {
  $buf = New-Object byte[] ($w * $h * 4)
  for ($y = 0; $y -lt $h; $y++) {
    for ($x = 0; $x -lt $w; $x++) {
      $dx = $x + 0.5 - $cx; $dy = $y + 0.5 - $cy
      $d = [Math]::Sqrt($dx * $dx + $dy * $dy)
      $cov = [Math]::Min(1.0, [Math]::Max(0.0, $rad - $d + 0.5))
      $i = ($y * $w + $x) * 4
      $buf[$i]     = [byte]($CB * $cov)
      $buf[$i + 1] = [byte]($CG * $cov)
      $buf[$i + 2] = [byte]($CR * $cov)
      $buf[$i + 3] = [byte](255 * $cov)
    }
  }
  return ,$buf
}

# 屏幕截图存 PNG
function Shot([int]$x, [int]$y, [int]$w, [int]$h, [string]$path) {
  $bmp = New-Object System.Drawing.Bitmap($w, $h)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size($w, $h)))
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}

# 边缘平滑度：水平扫描圆的左缘，统计"过渡像素"数量（0=硬锯齿）
function EdgeProfile([string]$path, [int]$y, [int]$x0, [int]$x1) {
  $b = [System.Drawing.Bitmap]::FromFile($path)
  $vals = @()
  for ($x = $x0; $x -le $x1; $x++) {
    $p = $b.GetPixel($x, $y)
    $vals += , @($p.R, $p.G, $p.B)
  }
  $b.Dispose()
  # 背景色 = 第一个像素；前景 = 最后一个像素；统计严格介于两者之间的像素数
  $bg = $vals[0]; $fg = $vals[$vals.Count - 1]
  $mid = 0; $s = ""; $n = 0
  foreach ($v in $vals) {
    $dB = [Math]::Abs($v[0] - $bg[0]) + [Math]::Abs($v[1] - $bg[1]) + [Math]::Abs($v[2] - $bg[2])
    $dF = [Math]::Abs($v[0] - $fg[0]) + [Math]::Abs($v[1] - $fg[1]) + [Math]::Abs($v[2] - $fg[2])
    if ($dB -gt 6 -and $dF -gt 6) { $mid++ }
    if ($n -lt 24) { $s += "({0},{1},{2})" -f $v[0], $v[1], $v[2] }
    $n++
  }
  return "过渡像素=$mid  前24像素=$s"
}

$S = $Size
$bytes = MakeCircleBytes $S $S ($S/2) ($S/2) ($S/2 - 12) 23 25 33
$wx = 200; $wy = 120

# ============ 阶段 1：顶层分层窗口 ============
$h = [LP]::Create($wx, $wy, $S, $S, 0x00080000 -bor 0x00000080 -bor 0x08000000 -bor 0x00000008, "LayerProbeA") # +WS_EX_TOPMOST
Write-Host "[1] 创建窗口 hwnd=$h  Rgn=$([LP]::RgnBox($h))"
$ok = [LP]::Present($h, $bytes, $S, $S, $wx, $wy)
Write-Host ("[Present] ok=$ok err={0}" -f [LP]::PresentErr)
[LP]::ShowWindow($h, 8) # SW_SHOWNA
[LP]::Pump(400)
$shot1 = "E:\tools\ring-dock\target\probe_toplevel.png"
Shot $wx $wy $S $S $shot1
# 圆的左缘在窗口内 x≈12，扫描 y=中线偏下 40px（斜边处才能看出锯齿）
Write-Host ("[Q1] 顶层边缘平滑度(斜边) " + (EdgeProfile $shot1 ([int]($S/2) + 40) 2 ($S/2)))
# 先显示桌面，避免真实鼠标点击落到用户其他窗口上（结尾还原）
[KD]::WinD()
Start-Sleep -Seconds 1
[LP]::ShowWindow($h, 8)
[LP]::Pump(200)
# 实心区点击
[LP]::RealClick([int]($wx + $S/2), [int]($wy + $S/2))
Write-Host ("[Q2a] 实心区点击 Clicks={0} LastClick={1} (期望 1)" -f [LP]::Clicks, [LP]::LastClick)
# 窗口内但圆外点击（圆缘距窗口边 12px）
[LP]::RealClick(($wx + 3), ($wy + 3))
Write-Host ("[Q2b] 圆外(透明)区点击 Clicks={0} (期望 0 = 穿透)" -f [LP]::Clicks)

# ============ 阶段 2：SetWindowRgn 是否裁剪分层显示 ============
[LP]::SetRgnEllipse($h, 40, 40, $S - 40, $S - 40)
[LP]::Pump(300)
$shot2 = "E:\tools\ring-dock\target\probe_rgn.png"
Shot $wx $wy $S $S $shot2
Write-Host ("[Q4] 套小 Rgn 后 RgnBox={0}" -f [LP]::RgnBox($h))
Write-Host ("[Q4] 套小 Rgn 后边缘序列（若圆被裁 → Rgn 生效） " + (EdgeProfile $shot2 ([int]($S/2) + 40) 2 ($S/2)))
[LP]::ClearRgn($h)
[LP]::Present($h, $bytes, $S, $S, $wx, $wy)
[LP]::Pump(200)

# ============ 阶段 3：SetParent 进桌面树 ============
$envInfo = [LP]::EmbedEx($h, $true)
Write-Host "[3] $envInfo"
[LP]::Present($h, $bytes, $S, $S, $wx, $wy)
[LP]::ShowWindow($h, 8)
[LP]::Pump(300)
# 嵌入后用真实屏幕坐标（子窗口坐标系已变为相对父窗口）
$wr = [LP]::WinRect($h) -split ","
$ex = [int]$wr[0]; $ey = [int]$wr[1]
Write-Host ("[3] 实际屏幕矩形=" + ([LP]::WinRect($h)))
$shot3 = "E:\tools\ring-dock\target\probe_embedded.png"
Shot $ex $ey $S $S $shot3
Write-Host ("[Q3a] 桌面子窗口边缘平滑度(斜边) " + (EdgeProfile $shot3 ([int]($S/2) + 40) 2 ($S/2)))
[LP]::RealClick([int]($ex + $S/2), [int]($ey + $S/2))
Write-Host ("[Q3b] 桌面子窗口实心区点击 Clicks={0} (期望 1)" -f [LP]::Clicks)
[LP]::RealClick(($ex + 3), ($ey + 3))
Write-Host ("[Q3c] 桌面子窗口透明区点击 Clicks={0} (期望 0)" -f [LP]::Clicks)
[KD]::WinD()
Start-Sleep -Milliseconds 800
Write-Host "探针完成（已还原桌面）"

# ============ 阶段 4：只 SetParent、保留 WS_POPUP（不转 WS_CHILD）=============
$h2 = [LP]::Create($wx, $wy, $S, $S, 0x00080000 -bor 0x00000080 -bor 0x08000000, "LayerProbeB")
[LP]::Present($h2, $bytes, $S, $S, $wx, $wy)
Write-Host ("[4] popup+SetParent: " + [LP]::EmbedEx($h2, $false))
[LP]::Present($h2, $bytes, $S, $S, $wx, $wy)
[LP]::ShowWindow($h2, 8)
[LP]::Pump(300)
[KD]::WinD()
Start-Sleep -Seconds 1
[LP]::ShowWindow($h2, 8)
[LP]::Pump(200)
$wr2 = [LP]::WinRect($h2) -split ","
Write-Host ("[4] 实际屏幕矩形=" + ([LP]::WinRect($h2)))
$shot4 = "E:\tools\ring-dock\target\probe_popup_parent.png"
Shot ([int]$wr2[0]) ([int]$wr2[1]) $S $S $shot4
Write-Host ("[Q4b] popup+SetParent 边缘平滑度(斜边) " + (EdgeProfile $shot4 ([int]($S/2) + 40) 2 ($S/2)))
[LP]::RealClick([int]([int]$wr2[0] + $S/2), [int]([int]$wr2[1] + $S/2))
Write-Host ("[Q4c] popup+SetParent 实心区点击 Clicks={0} (期望 1)" -f [LP]::Clicks)
[LP]::RealClick(([int]$wr2[0] + 3), ([int]$wr2[1] + 3))
Write-Host ("[Q4d] popup+SetParent 透明区点击 Clicks={0} (期望 0)" -f [LP]::Clicks)
[KD]::WinD()
Start-Sleep -Milliseconds 800
Write-Host "全部探针完成"
