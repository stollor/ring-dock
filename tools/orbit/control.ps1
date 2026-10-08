# PID-scoped Win32 test helper. Never selects an unrelated or old ring-dock instance.
Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class OrbitControl {
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
 public struct RECT{public int l,t,r,b;}
 public static string Class(IntPtr h){var s=new StringBuilder(256);GetClassNameW(h,s,256);return s.ToString();}
 public delegate bool EnumProc(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h,EnumProc f,IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int n);
 [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
 [DllImport("user32.dll",EntryPoint="ClientToScreen")] static extern bool NativeClientToScreen(IntPtr h,ref POINT p);
 [DllImport("user32.dll")] static extern bool SystemParametersInfoW(uint a,uint b,out RECT r,uint f);
 // Regression fixtures and diagnostic metadata use work-area coordinates.
 public static bool ClientToScreen(IntPtr h,ref POINT p){RECT r;SystemParametersInfoW(48,0,out r,0);p.x+=r.l;p.y+=r.t;return true;}
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr z,int x,int y,int w,int b,uint f);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] static extern IntPtr SendMessageTimeoutW(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr result);
 [DllImport("user32.dll")] public static extern int GetWindowLongW(IntPtr h,int n);
 [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [StructLayout(LayoutKind.Sequential)] struct GUIINFO {public uint cb,flags;public IntPtr active,focus,capture,menu,move,caret;public RECT rect;}
 [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint thread,ref GUIINFO info);
 static bool Captured(IntPtr h){uint pid;uint thread=GetWindowThreadProcessId(h,out pid);GUIINFO info=new GUIINFO();info.cb=(uint)Marshal.SizeOf(info);return GetGUIThreadInfo(thread,ref info)&&info.capture==h;}
 static void WaitCapture(IntPtr h,bool expected){var timer=System.Diagnostics.Stopwatch.StartNew();while(Captured(h)!=expected){if(timer.ElapsedMilliseconds>2000)throw new Exception("Mouse capture transition timed out");System.Threading.Thread.Sleep(5);}}
 static POINT savedCursor; static bool cursorSaved;
 [DllImport("user32.dll")] static extern void mouse_event(uint f,uint x,uint y,uint data,UIntPtr extra);
 public struct POINT{public int x,y; public POINT(int x,int y){this.x=x;this.y=y;}}
 public static IntPtr Find(int pid,string cls="RingDockVisual"){
  IntPtr found=IntPtr.Zero;
  EnumProc cb=(h,p)=>{uint id;GetWindowThreadProcessId(h,out id);var s=new StringBuilder(256);GetClassNameW(h,s,256);if(id==pid&&s.ToString()==cls){found=h;return false;}return true;};
  EnumWindows((h,p)=>{cb(h,p);if(found==IntPtr.Zero)EnumChildWindows(h,cb,p);return found==IntPtr.Zero;},IntPtr.Zero);return found;
 }
 public static string Title(IntPtr h){var s=new StringBuilder(256);GetWindowTextW(h,s,256);return s.ToString();}
 public static void Message(IntPtr h,uint m,int w,int l){
  if(m==0x207 || (m==0x200 && Captured(h))){
   if(!cursorSaved){GetCursorPos(out savedCursor);cursorSaved=true;}
   POINT target=new POINT((short)(l&65535),(short)((l>>16)&65535));ClientToScreen(h,ref target);SetCursorPos(target.x,target.y);
   // Moving a compact captured window generates a native WM_MOUSEMOVE; do not
   // inject a second message encoded against an origin that may already have moved.
   if(m==0x200)return;
  }
  if(m==0x201||m==0x202||m==0x200||m==0x207||m==0x208){
   POINT origin=new POINT();NativeClientToScreen(h,ref origin);RECT work;SystemParametersInfoW(48,0,out work,0);
   l=Pack((short)(l&65535)+work.l-origin.x,(short)((l>>16)&65535)+work.t-origin.y);
  }
  IntPtr r;if(SendMessageTimeoutW(h,m,(IntPtr)w,(IntPtr)l,2,3000,out r)==IntPtr.Zero)throw new Exception("SendMessageTimeout failed");
  if(m==0x111 && w==103 && cursorSaved){SetCursorPos(savedCursor.x,savedCursor.y);cursorSaved=false;}
 }
 public static int Pack(int x,int y){return ((y&65535)<<16)|(x&65535);}
 public static void Click(IntPtr h,int x,int y){Message(h,0x201,1,Pack(x,y));Message(h,0x202,0,Pack(x,y));}
 public static void RealMiddleDrag(IntPtr h,int x,int y,int dx,int dy){
  POINT old;GetCursorPos(out old);POINT start=new POINT(x,y);ClientToScreen(h,ref start);
  if(WindowFromPoint(start)!=h)throw new Exception("Middle-drag start is not our PID-scoped window");
  try{SetCursorPos(start.x,start.y);mouse_event(0x20,0,0,0,UIntPtr.Zero);
   System.Threading.Thread.Sleep(80);SetCursorPos(start.x+dx,start.y+dy);System.Threading.Thread.Sleep(120);
  }finally{mouse_event(0x40,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(100);SetCursorPos(old.x,old.y);}
 }
 public static void RealClick(IntPtr h,int x,int y){POINT old;GetCursorPos(out old);POINT pt=new POINT(x,y);ClientToScreen(h,ref pt);try{if(!SetCursorPos(pt.x,pt.y))throw new Exception("SetCursorPos failed");POINT actual;GetCursorPos(out actual);if(WindowFromPoint(actual)!=h)throw new Exception("Click target is not our PID-scoped window");mouse_event(2,0,0,0,UIntPtr.Zero);WaitCapture(h,true);mouse_event(4,0,0,0,UIntPtr.Zero);WaitCapture(h,false);}finally{mouse_event(4,0,0,0,UIntPtr.Zero);SetCursorPos(old.x,old.y);}}
}
"@
