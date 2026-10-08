# 编辑态端到端：长按进入编辑 → 点删除角标删条目 → 拖动条目换位 → 点空白退出编辑
# 观测：窗口标题（#expanded=N#edit）+ config.json 条目数量/顺序；结束后恢复配置
param()
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class E {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int m);
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
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  public struct POINT { public int x, y; }
  public static string Cursor() { POINT p; GetCursorPos(out p); return p.x + "," + p.y; }
  public static string Title(IntPtr h) { var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256); return sb.ToString(); }
  static IntPtr Lp(int x, int y) { return (IntPtr)(((y & 0xFFFF) << 16) | (x & 0xFFFF)); }
  public static void Down(IntPtr h, int x, int y) { SendMessageW(h, 0x0201, (IntPtr)1, Lp(x, y)); }
  public static void Up(IntPtr h, int x, int y) { SendMessageW(h, 0x0202, (IntPtr)0, Lp(x, y)); }
  public static void Move(IntPtr h, int x, int y) { SendMessageW(h, 0x0200, (IntPtr)1, Lp(x, y)); }
  public static void Click(IntPtr h, int x, int y) { Down(h, x, y); Up(h, x, y); }
}
"@
Add-Type -TypeDefinition $src

function Fail($msg) {
  Write-Host "[FAIL] $msg"
  if ($script:proc -and -not $script:proc.HasExited) { Stop-Process -Id $script:proc.Id -Force -ErrorAction SilentlyContinue }
  Set-Content $cfgPath $script:backup -Encoding UTF8
  exit 1
}

$cfgPath = "E:\tools\ring-dock\target\release\config.json"
$script:backup = Get-Content $cfgPath -Raw
$script:proc = Start-Process -FilePath "E:\tools\ring-dock\target\release\ring-dock.exe" -PassThru
Start-Sleep -Seconds 2

$h = [E]::FindAny("RingDockVisual")
if ($h -eq [IntPtr]::Zero) { Fail "找不到主窗口" }
$wa = [E]::WorkArea()
$cx = [int]($wa[2] / 2); $cy = [int]($wa[3] / 2)

# 布局参数（与 config 默认一致：icon=48 gap=12 pad=16）
$pad = 16; $icon = 48; $gap = 12
$layX = $cx; $layY = $cy - 180   # 面板0：圆心锚点、右上展开（6 条目 5 列 2 行 → 320x180）

# 1. 展开面板 0
$arc0x = [int]($cx + 117 * [Math]::Cos(-45 * [Math]::PI / 180)); $arc0y = [int]($cy + 117 * [Math]::Sin(-45 * [Math]::PI / 180))
[E]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 500
if ([E]::Title($h) -notmatch "expanded=0") { Fail "展开失败：$([E]::Title($h))" }
Write-Host "[PASS] 面板展开"

# 2. 长按面板空白（右内边距处）→ 编辑态（长按判定用真实鼠标位置 → 先把光标移过去）
$blankX = $layX + 312; $blankY = $cy - 100
[E]::SetCursorPos(($wa[0] + $blankX), ($wa[1] + $blankY)); Start-Sleep -Milliseconds 200
Write-Host ("[光标] 目标=" + ($wa[0] + $blankX) + "," + ($wa[1] + $blankY) + " 实际=" + [E]::Cursor())
[E]::Down($h, $blankX, $blankY)
Start-Sleep -Milliseconds 1200          # 超过 HOLD_MS=550
$t = [E]::Title($h)
if ($t -notmatch "#edit") { Fail "长按未进入编辑态：$t" }
Write-Host "[PASS] 长按进入编辑态（$t）"
[E]::Up($h, $blankX, $blankY); Start-Sleep -Milliseconds 300

# 3. 删除第 0 个条目（点它的删除角标）
$before = (Get-Content $cfgPath -Raw | ConvertFrom-Json).quadrants[0].items.Count
$badgeX = $layX + $pad + $icon - 4; $badgeY = $layY + $pad + 4
[E]::Click($h, $badgeX, $badgeY); Start-Sleep -Milliseconds 500
$after = (Get-Content $cfgPath -Raw | ConvertFrom-Json).quadrants[0].items.Count
if ($after -ne $before - 1) { Fail "删除未生效：$before → $after" }
Write-Host "[PASS] 删除角标生效（$before → $after）"

# 4. 拖动第 0 个条目到第 2 格 → 换位
$names0 = @((Get-Content $cfgPath -Raw | ConvertFrom-Json).quadrants[0].items | ForEach-Object { $_.name })
$c0x = $layX + $pad + [int]($icon / 2); $c0y = $layY + $pad + [int]($icon / 2)
$c2x = $layX + $pad + 2 * ($icon + $gap) + [int]($icon / 2); $c2y = $c0y
Write-Host ("[拖动] 从 ($c0x,$c0y) 到 ($c2x,$c2y)")
[E]::Down($h, $c0x, $c0y); Start-Sleep -Milliseconds 150
[E]::Move($h, [int](($c0x + $c2x) / 2), $c0y); Start-Sleep -Milliseconds 150
[E]::Move($h, $c2x, $c2y); Start-Sleep -Milliseconds 150
[E]::Up($h, $c2x, $c2y); Start-Sleep -Milliseconds 500
$names1 = @((Get-Content $cfgPath -Raw | ConvertFrom-Json).quadrants[0].items | ForEach-Object { $_.name })
if ($names1[2] -ne $names0[0]) { Fail "拖动换位未生效：原第0=$($names0[0]) 现第2=$($names1[2])（顺序：$($names1 -join ',')）" }
Write-Host "[PASS] 拖动换位生效（第0→第2：$($names0[0])）"

# 5. 点面板空白 → 退出编辑态
[E]::Click($h, $blankX, $blankY); Start-Sleep -Milliseconds 400
$t = [E]::Title($h)
if ($t -match "#edit") { Fail "未退出编辑态：$t" }
Write-Host "[PASS] 点空白退出编辑态（$t）"

Stop-Process -Id $script:proc.Id -Force -ErrorAction SilentlyContinue
Set-Content $cfgPath $script:backup -Encoding UTF8
Write-Host "[OK] 编辑态端到端全部通过（配置已恢复）"
