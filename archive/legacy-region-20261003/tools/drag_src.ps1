# OLE 拖源小工具：把一个文件用真实 DoDragDrop 拖到屏幕指定点（必须 STA 运行）
# 用法：powershell -STA -File drag_src.ps1 <文件> <x> <y>
param([string]$Path, [int]$X, [int]$Y)
$src = @"
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("00000121-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IDropSource {
  [PreserveSig] int QueryContinueDrag(int fEscapePressed, uint grfKeyState);
  [PreserveSig] int GiveFeedback(uint dwEffect);
}
[ComImport, Guid("0000010e-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IDataObj {
  [PreserveSig] int GetData(ref FORMATETC pFormatetcIn, out STGMEDIUM pMedium);
  [PreserveSig] int GetDataHere(ref FORMATETC pFormatetc, ref STGMEDIUM pMedium);
  [PreserveSig] int QueryGetData(ref FORMATETC pFormatetc);
  [PreserveSig] int GetCanonicalFormatEtc(ref FORMATETC pFormatetcIn, out FORMATETC pFormatetcOut);
  [PreserveSig] int SetData(ref FORMATETC pFormatetc, ref STGMEDIUM pMedium, [MarshalAs(UnmanagedType.Bool)] bool fRelease);
  [PreserveSig] int EnumFormatEtc(uint dwDirection, out IntPtr ppenumFormatEtc);
  [PreserveSig] int DAdvise(ref FORMATETC pFormatetc, uint advf, IntPtr pAdvSink, out uint pdwConnection);
  [PreserveSig] int DUnadvise(uint dwConnection);
  [PreserveSig] int EnumDAdvise(out IntPtr ppenumAdvise);
}
[StructLayout(LayoutKind.Sequential)] public struct FORMATETC { public ushort cfFormat; public IntPtr ptd; public uint dwAspect; public short lindex; public uint tymed; }
[StructLayout(LayoutKind.Sequential)] public struct STGMEDIUM { public uint tymed; public IntPtr unionMember; public IntPtr pUnkForRelease; }

public class DropSrc : IDropSource {
  public int QueryContinueDrag(int fEscapePressed, uint grfKeyState) { return fEscapePressed != 0 ? 1 : 0; }
  public int GiveFeedback(uint dwEffect) { return 0; }
}
public class FileData : IDataObj {
  string path;
  public FileData(string p) { path = p; }
  [DllImport("kernel32.dll")] static extern IntPtr GlobalAlloc(uint flags, IntPtr size);
  [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr h);
  [DllImport("kernel32.dll")] static extern int GlobalUnlock(IntPtr h);
  [DllImport("shell32.dll")] static extern int SHCreateStdEnumFmtEtc(uint cfmt, FORMATETC[] afmt, out IntPtr ppenum);
  IntPtr BuildHDrop() {
    byte[] head = new byte[20];
    BitConverter.GetBytes(20u).CopyTo(head, 0);
    BitConverter.GetBytes(1).CopyTo(head, 16); // fWide
    byte[] names = System.Text.Encoding.Unicode.GetBytes(path + "\0\0");
    IntPtr h = GlobalAlloc(0x0042, (IntPtr)(20 + names.Length));
    IntPtr p = GlobalLock(h);
    Marshal.Copy(head, 0, p, 20);
    Marshal.Copy(names, 0, new IntPtr(p.ToInt64() + 20), names.Length);
    GlobalUnlock(h);
    return h;
  }
  public int GetData(ref FORMATETC f, out STGMEDIUM m) {
    m = new STGMEDIUM();
    if (f.cfFormat != 15) return -2147221404;
    m.tymed = 1; m.unionMember = BuildHDrop();
    return 0;
  }
  public int GetDataHere(ref FORMATETC f, ref STGMEDIUM m) { return -2147467263; }
  public int QueryGetData(ref FORMATETC f) { return f.cfFormat == 15 ? 0 : -2147221404; }
  public int GetCanonicalFormatEtc(ref FORMATETC a, out FORMATETC b) { b = new FORMATETC(); return -2147467263; }
  public int SetData(ref FORMATETC a, ref STGMEDIUM b, bool c) { return -2147467263; }
  public int EnumFormatEtc(uint d, out IntPtr e) {
    FORMATETC f = new FORMATETC(); f.cfFormat = 15; f.dwAspect = 1; f.lindex = -1; f.tymed = 1;
    return SHCreateStdEnumFmtEtc(1, new FORMATETC[] { f }, out e);
  }
  public int DAdvise(ref FORMATETC a, uint b, IntPtr c, out uint d) { d = 0; return -2147467263; }
  public int DUnadvise(uint a) { return -2147467263; }
  public int EnumDAdvise(out IntPtr a) { a = IntPtr.Zero; return -2147467263; }
}
public class Dragger {
  [DllImport("ole32.dll")] static extern int OleInitialize(IntPtr p);
  [DllImport("ole32.dll")] static extern int DoDragDrop(IntPtr d, IntPtr s, uint fx, out uint effect);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  public static string Run(string path, int x, int y) {
    int oi = OleInitialize(IntPtr.Zero);
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(150);
    // DoDragDrop 的模态循环等鼠标输入才会推进 → 后台线程经微移动鼠标
    var t = new System.Threading.Thread(() => {
      for (int i = 0; i < 30; i++) {
        SetCursorPos(x + (i % 3), y + (i % 2));
        System.Threading.Thread.Sleep(100);
      }
    });
    t.IsBackground = true;
    t.Start();
    IntPtr d = Marshal.GetIUnknownForObject(new FileData(path));
    IntPtr s = Marshal.GetIUnknownForObject(new DropSrc());
    uint effect;
    int hr = DoDragDrop(d, s, 1, out effect);
    Marshal.Release(d); Marshal.Release(s);
    return "oleinit=0x" + oi.ToString("X8") + " hr=0x" + hr.ToString("X8") + " effect=" + effect;
  }
}
"@
Add-Type -TypeDefinition $src
[Dragger]::Run($Path, $X, $Y)
