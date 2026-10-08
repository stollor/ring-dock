# 拖拽收纳端到端实测：STA 子进程起真实 OLE DoDragDrop，把文件拖到展开的面板上
# 验证：拖入后 config.json 该象限条目 +1（名称/类型推断正确），结束后恢复配置
param()
$ErrorActionPreference = "Continue"

$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class F {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SystemParametersInfoW(uint a, uint b, out RECT rc, uint f);
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
}
"@
Add-Type -TypeDefinition $src

$cfgPath = "E:\tools\ring-dock\target\release\config.json"
$backup = Get-Content $cfgPath -Raw
$dropFile = "E:\tools\ring-dock\target\drop_test.txt"
Set-Content $dropFile "drag-drop e2e test" -Encoding UTF8

$p = Start-Process "E:\tools\ring-dock\target\release\ring-dock.exe" -PassThru
Start-Sleep -Seconds 2

$h = [F]::FindAny("RingDockVisual")
if ($h -eq [IntPtr]::Zero) {
  Write-Host "[FAIL] 找不到窗口"
  Stop-Process -Id $p.Id -Force; Set-Content $cfgPath $backup -Encoding UTF8; Remove-Item $dropFile -Force; exit 1
}
$wa = [F]::WorkArea()
$cx = [int]($wa[2] / 2); $cy = [int]($wa[3] / 2)
$arc0x = [int]($cx + 117 * [Math]::Cos(-45 * [Math]::PI / 180)); $arc0y = [int]($cy + 117 * [Math]::Sin(-45 * [Math]::PI / 180))
$lp = [IntPtr]((($arc0y -band 0xFFFF) -shl 16) -bor ($arc0x -band 0xFFFF))
[F]::SendMessageW($h, 0x0201, [IntPtr]1, $lp) | Out-Null
[F]::SendMessageW($h, 0x0202, [IntPtr]0, $lp) | Out-Null
Start-Sleep -Milliseconds 800

$before = (Get-Content $cfgPath -Raw | ConvertFrom-Json).quadrants[0].items.Count
$dropX = $cx + 160; $dropY = $cy - 90   # 面板（圆心锚点、右上展开）中部
Write-Host "[基线] 象限0 条目数=$before   [拖到] ($dropX,$dropY)"

# STA 子进程跑拖源（默认 MTA 会让 DoDragDrop 挂住）；20s 超时保护
$pr = Start-Process powershell -ArgumentList "-STA", "-ExecutionPolicy", "Bypass", "-File",
  "E:\tools\ring-dock\tools\drag_src.ps1", $dropFile, "$dropX", "$dropY" -PassThru -NoNewWindow
if (-not $pr.WaitForExit(20000)) {
  Write-Host "[超时] DoDragDrop 20s 未返回，杀掉拖源进程"
  Stop-Process -Id $pr.Id -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Seconds 1

$cfg = Get-Content $cfgPath -Raw | ConvertFrom-Json
$after = $cfg.quadrants[0].items.Count
$last = $cfg.quadrants[0].items[$after - 1]
Write-Host "[结果] 象限0 条目数=$after（期望 $($before + 1)）  新条目名=$($last.name) kind=$($last.kind)"

Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Set-Content $cfgPath $backup -Encoding UTF8
Remove-Item $dropFile -Force -ErrorAction SilentlyContinue
if ($after -eq $before + 1 -and $last.kind -eq "file") {
  Write-Host "[PASS] 拖拽收纳端到端通过（配置已恢复）"
} else {
  Write-Host "[FAIL] 拖拽未生效（配置已恢复）"
  exit 1
}
