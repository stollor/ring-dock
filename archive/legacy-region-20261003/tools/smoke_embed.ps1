$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public delegate bool EnumProc(IntPtr h, IntPtr l);
public class W {
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern int GetWindowLongW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr ex);

  public static string ClassOf(IntPtr h) {
    var sb = new StringBuilder(256);
    GetClassNameW(h, sb, 256);
    return sb.ToString();
  }
  // 在整个窗口树（顶层 + 所有子窗口）里按类名找窗口
  public static IntPtr FindAny(string cls) {
    IntPtr found = IntPtr.Zero;
    EnumProc childCb = null;
    childCb = (h, l) => {
      if (ClassOf(h) == cls) { found = h; return false; }
      return true;
    };
    EnumProc topCb = null;
    topCb = (h, l) => {
      if (ClassOf(h) == cls) { found = h; return false; }
      EnumChildWindows(h, childCb, IntPtr.Zero);
      return found == IntPtr.Zero;
    };
    EnumWindows(topCb, IntPtr.Zero);
    return found;
  }
  public static string Chain(IntPtr h) {
    var parts = new System.Collections.Generic.List<string>();
    for (int i = 0; i < 8 && h != IntPtr.Zero; i++) {
      parts.Add(ClassOf(h));
      h = GetParent(h);
    }
    return string.Join(" <- ", parts);
  }
  public static long Style(IntPtr h) { return GetWindowLongW(h, -16) & 0xFFFFFFFFL; }
  public static long ExStyle(IntPtr h) { return GetWindowLongW(h, -20) & 0xFFFFFFFFL; }
  public static string State(IntPtr h) {
    return string.Format("IsWindow={0} IsWindowVisible={1} IsIconic={2}",
      IsWindow(h), IsWindowVisible(h), IsIconic(h));
  }
  public static void WinD() {
    keybd_event(0x5B, 0, 0, IntPtr.Zero);   // Win down
    keybd_event(0x44, 0, 0, IntPtr.Zero);   // D down
    keybd_event(0x44, 0, 2, IntPtr.Zero);   // D up
    keybd_event(0x5B, 0, 2, IntPtr.Zero);   // Win up
  }
}
"@
Add-Type -TypeDefinition $src

$proc = Start-Process -FilePath "E:\tools\ring-dock\target\debug\ring-dock.exe" -PassThru `
        -RedirectStandardError "E:\tools\ring-dock\target\rd-stderr.log"
Start-Sleep -Seconds 3

$h = [W]::FindAny("RingDockVisual")
if ($h -eq [IntPtr]::Zero) {
  Write-Host "[FAIL] 窗口树里找不到 RingDockVisual"
  Write-Host "--- stderr ---"; Get-Content "E:\tools\ring-dock\target\rd-stderr.log" -ErrorAction SilentlyContinue
  Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
  exit 1
}
$style = [W]::Style($h); $ex = [W]::ExStyle($h)
Write-Host "[窗口层级] $([W]::Chain($h))"
Write-Host ("[样式] WS_CHILD={0} WS_POPUP={1} WS_EX_TOPMOST={2}" -f `
  (($style -band 0x40000000) -ne 0), (($style -band 0x80000000) -ne 0), (($ex -band 0x00080000) -ne 0))
Write-Host "[Win+D 前] $([W]::State($h))"

[W]::WinD()                       # 显示桌面
Start-Sleep -Seconds 2
Write-Host "[Win+D 后] $([W]::State($h))"

[W]::WinD()                       # 再按一次还原
Start-Sleep -Seconds 1

Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Write-Host "--- stderr ---"; Get-Content "E:\tools\ring-dock\target\rd-stderr.log" -ErrorAction SilentlyContinue
Write-Host "[OK] 测试完成，ring-dock 已退出"
