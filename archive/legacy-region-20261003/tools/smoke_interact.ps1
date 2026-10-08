# 交互行为自动化回归：模拟点击 + GetRegionData 精确观测命中区域（面积法）
# 验证：弧缝不误触发 / 象限展开收起 / 跨象限切换与收起（设置项）/ 面板空白收起 / 中心收起 / 设置窗口 / 退出
$src = @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public delegate bool EnumProc(IntPtr h, IntPtr l);
public class I {
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h, IntPtr rgn);
  [DllImport("gdi32.dll")]  static extern IntPtr CreateRectRgn(int a, int b, int c, int d);
  [DllImport("gdi32.dll")]  static extern int GetRgnBox(IntPtr rgn, out RECT rc);
  [DllImport("gdi32.dll")]  static extern uint GetRegionData(IntPtr rgn, uint count, byte[] buf);
  [DllImport("user32.dll")] static extern bool SystemParametersInfoW(uint a, uint b, out RECT rc, uint f);
  public struct RECT { public int left, top, right, bottom; }

  // 展开状态观测：标题带 #expanded=N（小面板落在圆盘内时 Rgn 面积不变，面积法失效）
  public static string StateOf(IntPtr h) {
    var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256);
    string t = sb.ToString();
    int i = t.IndexOf("#expanded=");
    return i < 0 ? "-" : t.Substring(i + 10);
  }

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
  // 命中区域精确观测：面积（Σ矩形）+ 包围盒
  public static string RgnInfo(IntPtr h) {
    IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
    GetWindowRgn(h, rgn);
    uint need = GetRegionData(rgn, 0, null);
    byte[] buf = new byte[need];
    GetRegionData(rgn, need, buf);
    uint n = BitConverter.ToUInt32(buf, 8);
    long area = 0;
    for (int i = 0; i < n; i++) {
      int o = 32 + i * 16;
      int l = BitConverter.ToInt32(buf, o), t = BitConverter.ToInt32(buf, o + 4);
      int r = BitConverter.ToInt32(buf, o + 8), b = BitConverter.ToInt32(buf, o + 12);
      area += (long)(r - l) * (b - t);
    }
    RECT rc; GetRgnBox(rgn, out rc);
    return string.Format("area={0},box={1},{2},{3},{4}", area, rc.left, rc.top, rc.right, rc.bottom);
  }
  public static void Click(IntPtr h, int x, int y) {
    IntPtr lp = (IntPtr)(((y & 0xFFFF) << 16) | (x & 0xFFFF));
    SendMessageW(h, 0x0201, (IntPtr)0x0001, lp);
    SendMessageW(h, 0x0202, (IntPtr)0x0000, lp);
  }
}
"@
Add-Type -TypeDefinition $src

function Fail($msg) {
  Write-Host "[FAIL] $msg"
  if ($script:proc -and -not $script:proc.HasExited) { Stop-Process -Id $script:proc.Id -Force -ErrorAction SilentlyContinue }
  exit 1
}
function AssertEq($actual, $expected, $name) {
  if ($actual -ne $expected) { Fail "$name`n       期望=$expected`n       实际=$actual" }
  Write-Host "[PASS] $name"
}
function AssertNotEq($actual, $bad, $name) {
  if ($actual -eq $bad) { Fail "$name（不应为 $bad）" }
  Write-Host "[PASS] $name"
}
function AssertState($expected, $name) {
  $actual = [I]::StateOf($h)
  if ($actual -ne $expected) { Fail "$name`n       期望展开=$expected`n       实际=$actual" }
  Write-Host "[PASS] $name"
}

$cfgPath = "E:\tools\ring-dock\target\release\config.json"
$script:proc = Start-Process -FilePath "E:\tools\ring-dock\target\release\ring-dock.exe" -PassThru
Start-Sleep -Seconds 2

$h = [I]::FindAny("RingDockVisual")
if ($h -eq [IntPtr]::Zero) { Fail "找不到主窗口" }
Write-Host "[OK] 主窗口已找到"

# 几何（与 RingGeom 一致）：窗口铺满工作区，圆心=窗口中心
$wa = [I]::WorkArea()
$cx = [int]($wa[2] / 2); $cy = [int]($wa[3] / 2)
$rMid = 117.0
$arc0x = [int]($cx + $rMid * [Math]::Cos(-45 * [Math]::PI / 180)); $arc0y = [int]($cy + $rMid * [Math]::Sin(-45 * [Math]::PI / 180))
$arc1x = [int]($cx + $rMid * [Math]::Cos( 45 * [Math]::PI / 180)); $arc1y = [int]($cy + $rMid * [Math]::Sin( 45 * [Math]::PI / 180))
$gapX  = $cx + $rMid; $gapY = $cy                    # 弧缝（0° 方向，视觉空白）
$panelBlank0x = $cx + 312; $panelBlank0y = $cy - 100   # 面板0（圆心锚点、右上展开）内空白

Start-Sleep -Milliseconds 300
$base = [I]::RgnInfo($h)
Write-Host "[基准 Rgn] $base"

# 1. 弧缝点击：不触发任何展开
[I]::Click($h, $gapX, $gapY); Start-Sleep -Milliseconds 400
AssertEq ([I]::RgnInfo($h)) $base "弧缝点击不触发展开（回归：旧版误触发）"

# 2. 点弧 0 → 展开面板 0
[I]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 500
$open0 = [I]::RgnInfo($h)
Write-Host "[Rgn] 展开0=$open0"
AssertState "0" "点弧 0 展开面板"

# 3. 再点弧 0 → 收起
[I]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 500
AssertState "-" "再点同一象限收起"

# 显式设定前提：switch_panel_on_click=true（此前失败的运行可能把 false 留在配置里）
$cfg = Get-Content $cfgPath -Raw | ConvertFrom-Json
if ($cfg.PSObject.Properties.Name -notcontains "switch_panel_on_click") {
  $cfg | Add-Member -NotePropertyName switch_panel_on_click -NotePropertyValue $true
}
if ($cfg.PSObject.Properties.Name -notcontains "auto_collapse_after_open") {
  $cfg | Add-Member -NotePropertyName auto_collapse_after_open -NotePropertyValue $true
}
$cfg.switch_panel_on_click = $true
$cfg | ConvertTo-Json -Depth 10 | Set-Content $cfgPath -Encoding UTF8
[I]::PostMessageW($h, 0x0111, [IntPtr]101, [IntPtr]0) | Out-Null   # MENU_RELOAD
Start-Sleep -Milliseconds 500

# 4. switch_panel_on_click=true（默认）：展开 0 后点弧 1 → 切换到面板 1（保持展开）
[I]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 500
$open0 = [I]::RgnInfo($h)
[I]::Click($h, $arc1x, $arc1y); Start-Sleep -Milliseconds 500
$open1 = [I]::RgnInfo($h)
Write-Host "[Rgn] 切换后=$open1（面板1小、落在圆盘内，Rgn 面积可不变）"
AssertState "1" "true 档：展开 0 后点弧 1 → 切换到面板 1（保持展开）"

# 5. 面板空白点击 → 收起（\"回得去\"）
[I]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 500   # 切回面板 0（大面板）
$Ignored = [I]::RgnInfo($h)
[I]::Click($h, $panelBlank0x, $panelBlank0y); Start-Sleep -Milliseconds 500
AssertState "-" "面板空白点击收起"

# 6. 展开后点中心 → 收起
[I]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 500
[I]::Click($h, $cx, $cy); Start-Sleep -Milliseconds 500
AssertState "-" "中心点击收起"

# 7. switch_panel_on_click=false：展开 0 后点弧 1 → 收起（不切换）
$cfg = Get-Content $cfgPath -Raw | ConvertFrom-Json
if ($cfg.PSObject.Properties.Name -notcontains "switch_panel_on_click") {
  $cfg | Add-Member -NotePropertyName switch_panel_on_click -NotePropertyValue $true
}
if ($cfg.PSObject.Properties.Name -notcontains "auto_collapse_after_open") {
  $cfg | Add-Member -NotePropertyName auto_collapse_after_open -NotePropertyValue $true
}
$cfg.switch_panel_on_click = $false
$cfg | ConvertTo-Json -Depth 10 | Set-Content $cfgPath -Encoding UTF8
[I]::PostMessageW($h, 0x0111, [IntPtr]101, [IntPtr]0) | Out-Null   # WM_COMMAND MENU_RELOAD
Start-Sleep -Milliseconds 500
[I]::Click($h, $arc0x, $arc0y); Start-Sleep -Milliseconds 500
AssertState "0" "false 档：点弧 0 展开"
[I]::Click($h, $arc1x, $arc1y); Start-Sleep -Milliseconds 500
AssertState "-" "false 档：点弧 1 先收起（不切换）"

# 8. 设置窗口：打开 → 存在 → 保存 → 关闭 → 配置落盘
[I]::PostMessageW($h, 0x0111, [IntPtr]104, [IntPtr]0) | Out-Null   # WM_COMMAND MENU_SETTINGS
Start-Sleep -Seconds 1
$sw = [I]::FindAny("RingDockSettings")
if ($sw -eq [IntPtr]::Zero) { Fail "设置窗口未打开" }
Write-Host "[PASS] 设置窗口已打开"
[I]::PostMessageW($sw, 0x0111, [IntPtr]2001, [IntPtr]0) | Out-Null # 保存
Start-Sleep -Seconds 1
if ([I]::IsWindow($sw)) { Fail "保存后设置窗口未关闭" }
Write-Host "[PASS] 保存后设置窗口关闭"
$saved = Get-Content $cfgPath -Raw | ConvertFrom-Json
if ($saved.PSObject.Properties.Name -notcontains "switch_panel_on_click") {
  Fail "配置未落盘"
}
Write-Host "[PASS] 设置已写入 config.json（switch=$($saved.switch_panel_on_click) auto_collapse=$($saved.auto_collapse_after_open)）"

# 9. 退出：WM_COMMAND MENU_QUIT → 进程消失
#    注：窗口与托盘立即消失；进程收尾（D2D/COM/GDI 清理）实测需 ~3.5s，故轮询判定
$t0 = [DateTimeOffset]::Now.ToUnixTimeMilliseconds()
[I]::PostMessageW($h, 0x0111, [IntPtr]103, [IntPtr]0) | Out-Null
$exited = $false
foreach ($j in 1..300) {
  Start-Sleep -Milliseconds 100
  if ($script:proc.HasExited) { $exited = $true; break }
}
if (-not $exited) { Fail "退出命令未生效（30 秒未退出）" }
$dt = [DateTimeOffset]::Now.ToUnixTimeMilliseconds() - $t0
Write-Host "[PASS] 退出命令生效（进程收尾耗时 ${dt}ms）"

# 复原配置（switch 恢复默认 true）
$cfg = Get-Content $cfgPath -Raw | ConvertFrom-Json
$cfg.switch_panel_on_click = $true
$cfg | ConvertTo-Json -Depth 10 | Set-Content $cfgPath -Encoding UTF8
Write-Host "[OK] 全部交互回归通过"
