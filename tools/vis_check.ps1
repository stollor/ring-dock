# 视觉验收：PrintWindow 抓窗口自身渲染（不依赖桌面遮挡 / Win+D 状态）
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public delegate bool EnumProc(IntPtr h, IntPtr l);
public class V {
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT rc);
  [DllImport("gdi32.dll")]  static extern bool BitBlt(IntPtr dst, int x, int y, int w, int h, IntPtr src, int sx, int sy, uint rop);
  [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateCompatibleDC(IntPtr dc);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateCompatibleBitmap(IntPtr dc, int w, int h);
  [DllImport("gdi32.dll")]  static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
  [DllImport("gdi32.dll")]  static extern int GetDIBits(IntPtr dc, IntPtr bmp, uint s, uint l, byte[] buf, ref BI bi, uint u);
  [DllImport("gdi32.dll")]  static extern bool DeleteObject(IntPtr o);
  [DllImport("gdi32.dll")]  static extern bool DeleteDC(IntPtr dc);
  public struct RECT { public int left, top, right, bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct BIH { public uint s; public int w,h; public ushort p,b; public uint c,si; public int x,y; public uint cu,ci; }
  [StructLayout(LayoutKind.Sequential)] public struct BI { public BIH h; public uint col; }

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
  // 抓窗口自身绘制缓冲（GetDC+BitBlt：不经过屏幕合成，不怕遮挡）
  public static byte[] Shot(IntPtr h, out int w, out int ht) {
    RECT rc; GetClientRect(h, out rc);
    w = rc.right; ht = rc.bottom;
    IntPtr wdc = GetDC(h), memdc = CreateCompatibleDC(wdc);
    IntPtr bmp = CreateCompatibleBitmap(wdc, w, ht);
    IntPtr old = SelectObject(memdc, bmp);
    BitBlt(memdc, 0, 0, w, ht, wdc, 0, 0, 0x00CC0020);
    var bi = new BI(); bi.h.s = 40; bi.h.w = w; bi.h.h = -ht; bi.h.p = 1; bi.h.b = 24;
    byte[] buf = new byte[w * ht * 3 + w * 4];
    GetDIBits(memdc, bmp, 0, (uint)ht, buf, ref bi, 0);
    SelectObject(memdc, old); DeleteObject(bmp); DeleteDC(memdc); ReleaseDC(h, wdc);
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
  public static void Click(IntPtr h, int x, int y) {
    IntPtr lp = (IntPtr)(((y & 0xFFFF) << 16) | (x & 0xFFFF));
    SendMessageW(h, 0x0201, (IntPtr)0x0001, lp);
    SendMessageW(h, 0x0202, (IntPtr)0x0000, lp);
  }
}
"@
Add-Type -TypeDefinition $src

$p = Start-Process "E:\tools\ring-dock\target\release\ring-dock.exe" -PassThru
Start-Sleep -Seconds 2
$h = [V]::FindAny("RingDockVisual")
# 1. 圆环态
$w = 0; $ht = 0
$buf = [V]::Shot($h, [ref]$w, [ref]$ht)
# 从全窗口裁圆环区（窗口=工作区大小，圆心=窗口中心）
$cx = [int]($w/2); $cy = [int]($ht/2)
[V]::SaveBmp($buf, $w, $ht, "E:\tools\ring-dock\target\vis_ring_full.bmp")
# 2. 展开面板 0（右上）
$arc0x = [int]($cx + 117*[Math]::Cos(-45*[Math]::PI/180)); $arc0y = [int]($cy + 117*[Math]::Sin(-45*[Math]::PI/180))
[V]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 900
$w = 0; $ht = 0
$buf = [V]::Shot($h, [ref]$w, [ref]$ht)
[V]::SaveBmp($buf, $w, $ht, "E:\tools\ring-dock\target\vis_panel_full.bmp")
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Write-Host "抓取完成：vis_ring_full.bmp / vis_panel_full.bmp (${w}x${ht})"
