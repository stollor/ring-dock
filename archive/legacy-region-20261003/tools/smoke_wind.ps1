$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public delegate bool EnumProc(IntPtr h, IntPtr l);
public class V {
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowLongW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr ex);
  public static string ClassOf(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }
  // 列出顶层窗口状态：类名=最小化?/可见?（排除工具窗/桌面壳）
  public static string Snapshot() {
    var sb = new StringBuilder();
    EnumProc cb = null;
    cb = (h, l) => {
      long ex = GetWindowLongW(h, -20) & 0xFFFFFFFFL;
      bool tool = (ex & 0x00000080) != 0;
      string cls = ClassOf(h);
      if (!tool && cls != "Progman" && cls != "WorkerW" && cls != "Shell_TrayWnd") {
        sb.AppendFormat("{0}:iconic={1},visible={2}; ", cls, IsIconic(h), IsWindowVisible(h));
      }
      return true;
    };
    EnumWindows(cb, IntPtr.Zero);
    return sb.ToString();
  }
  public static void WinD() {
    keybd_event(0x5B, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 0, IntPtr.Zero);
    keybd_event(0x44, 0, 2, IntPtr.Zero);
    keybd_event(0x5B, 0, 2, IntPtr.Zero);
  }
}
"@
Add-Type -TypeDefinition $src

Write-Host "[Win+D 前] $([V]::Snapshot())"
[V]::WinD()
Start-Sleep -Seconds 2
Write-Host "[Win+D 后] $([V]::Snapshot())"
[V]::WinD()
Start-Sleep -Seconds 1
Write-Host "[还原后 ] $([V]::Snapshot())"
