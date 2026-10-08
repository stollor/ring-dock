// Diagnostic observer only: DWM desktop thumbnail + screenshots; never used by the renderer.
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using System.IO;
public class DesktopObserver {
 delegate IntPtr WndProc(IntPtr h,uint m,IntPtr w,IntPtr l);
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct WC {public uint style;public WndProc proc;public int a,b;public IntPtr inst,icon,cursor,brush;public string menu,name;}
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern ushort RegisterClassW(ref WC c);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern IntPtr CreateWindowExW(uint ex,string cls,string title,uint style,int x,int y,int w,int h,IntPtr parent,IntPtr menu,IntPtr inst,IntPtr data);
 [DllImport("user32.dll")]static extern IntPtr DefWindowProcW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")]static extern bool DestroyWindow(IntPtr h);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)]static extern IntPtr GetModuleHandleW(string n);
 static WndProc proc=DefWindowProcW;
 [DllImport("user32.dll")]static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int ht,uint flags);
 [StructLayout(LayoutKind.Sequential)]struct SIZE {public int x,y;}
 [StructLayout(LayoutKind.Sequential)]struct BLEND {public byte op,flags,alpha,format;}
 [StructLayout(LayoutKind.Sequential)]struct BI {public uint size;public int w,h;public ushort planes,bits;public uint compression,image;public int xp,yp;public uint used,important,color;}
 [DllImport("gdi32.dll")]static extern IntPtr CreateCompatibleDC(IntPtr dc);
 [DllImport("gdi32.dll")]static extern bool DeleteDC(IntPtr dc);
 [DllImport("gdi32.dll")]static extern IntPtr CreateDIBSection(IntPtr dc,ref BI info,uint usage,out IntPtr bits,IntPtr section,uint off);
 [DllImport("gdi32.dll")]static extern IntPtr SelectObject(IntPtr dc,IntPtr obj);
 [DllImport("gdi32.dll")]static extern bool DeleteObject(IntPtr obj);
 [DllImport("user32.dll")]static extern bool UpdateLayeredWindow(IntPtr h,IntPtr dst,ref POINT pos,ref SIZE size,IntPtr dc,ref POINT src,uint key,ref BLEND blend,uint flags);
 static void Backdrop(IntPtr h,POINT pos,int w,int ht,int red,int green,int blue){
  byte[] pixels=new byte[w*ht*4];for(int i=0;i<pixels.Length;i+=4){pixels[i]=(byte)blue;pixels[i+1]=(byte)green;pixels[i+2]=(byte)red;pixels[i+3]=255;}
  IntPtr dc=CreateCompatibleDC(IntPtr.Zero),bits;BI bi=new BI{size=40,w=w,h=-ht,planes=1,bits=32};IntPtr bmp=CreateDIBSection(dc,ref bi,0,out bits,IntPtr.Zero,0),old=SelectObject(dc,bmp);
  try{Marshal.Copy(pixels,0,bits,pixels.Length);SIZE size=new SIZE{x=w,y=ht};POINT src=new POINT();BLEND blend=new BLEND{alpha=255,format=1};if(!UpdateLayeredWindow(h,IntPtr.Zero,ref pos,ref size,dc,ref src,0,ref blend,2))throw new Exception("Backdrop presentation failed");}
  finally{SelectObject(dc,old);DeleteObject(bmp);DeleteDC(dc);}
 }


 [StructLayout(LayoutKind.Sequential)] struct RECT {public int l,t,r,b;public RECT(int x,int y,int w,int h){l=x;t=y;r=x+w;b=y+h;}}
 [StructLayout(LayoutKind.Sequential)] struct POINT {public int x,y;}
 [StructLayout(LayoutKind.Sequential)] struct THUMB {public uint flags;public RECT dest,source;public byte opacity;[MarshalAs(UnmanagedType.Bool)]public bool visible;[MarshalAs(UnmanagedType.Bool)]public bool client;}
 [DllImport("dwmapi.dll")]static extern int DwmRegisterThumbnail(IntPtr dest,IntPtr src,out IntPtr thumb);
 [DllImport("dwmapi.dll")]static extern int DwmUpdateThumbnailProperties(IntPtr thumb,ref THUMB p);
 [DllImport("dwmapi.dll")]static extern int DwmUnregisterThumbnail(IntPtr thumb);
 [DllImport("user32.dll")]static extern IntPtr GetParent(IntPtr h);
 [DllImport("user32.dll")]static extern bool GetWindowRect(IntPtr h,out RECT r);
 [DllImport("user32.dll")]static extern bool ClientToScreen(IntPtr h,ref POINT p);
 [DllImport("user32.dll")]static extern bool SystemParametersInfoW(uint a,uint b,out RECT r,uint f);
 [DllImport("user32.dll")]static extern bool ShowWindow(IntPtr h,int n);
 [DllImport("user32.dll")]static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")]static extern IntPtr WindowFromPoint(POINT p);
 [DllImport("user32.dll")]static extern IntPtr SetThreadDpiAwarenessContext(IntPtr p);
 static void Pump(int ms){DateTime end=DateTime.Now.AddMilliseconds(ms);while(DateTime.Now<end){Application.DoEvents();System.Threading.Thread.Sleep(10);}}
 static void Shot(IntPtr f,int w,int h,string path){POINT pt=new POINT{x=8,y=8};ClientToScreen(f,ref pt);if(WindowFromPoint(pt)!=f)throw new Exception("Observer not visible, refusing screenshot");using(Bitmap b=new Bitmap(w,h)){using(Graphics g=Graphics.FromImage(b)){g.CopyFromScreen(pt.x,pt.y,0,0,new Size(w,h));}b.Save(path,ImageFormat.Png);}}
 [STAThread]public static void Main(string[] a){
  SetThreadDpiAwarenessContext(new IntPtr(-4));
  IntPtr app=new IntPtr(long.Parse(a[0]));int x=int.Parse(a[1]),y=int.Parse(a[2]),w=int.Parse(a[3]),h=int.Parse(a[4]);string prefix=a[5];
  IntPtr root=app;while(GetParent(root)!=IntPtr.Zero)root=GetParent(root);
  if(root==app)throw new Exception("Not embedded: no desktop parent");
  RECT rr;GetWindowRect(root,out rr);RECT work;SystemParametersInfoW(48,0,out work,0);POINT pos=new POINT{x=x+work.l,y=y+work.t};
  WC wc=new WC{proc=proc,inst=GetModuleHandleW(null),brush=new IntPtr(5),name="OrbitDesktopObserver"};RegisterClassW(ref wc);
  IntPtr f=CreateWindowExW(0x08000088,"OrbitDesktopObserver","Orbit Glass verification",0x90000000,100,80,w+16,h+16,IntPtr.Zero,IntPtr.Zero,GetModuleHandleW(null),IntPtr.Zero);
  if(f==IntPtr.Zero)throw new Exception("Observer creation failed");
  IntPtr thumb=IntPtr.Zero,pad=IntPtr.Zero;bool visible=IsWindowVisible(app);bool controlled=a.Length>6;
  try{
   ShowWindow(f,4);Pump(100);
   int hr=DwmRegisterThumbnail(f,root,out thumb);if(hr!=0)throw new Exception("DwmRegisterThumbnail "+hr);
   THUMB t=new THUMB{flags=31,dest=new RECT(8,8,w,h),source=new RECT(pos.x-rr.l,pos.y-rr.t,w,h),opacity=255,visible=true,client=false};
   hr=DwmUpdateThumbnailProperties(thumb,ref t);if(hr!=0)throw new Exception("DwmUpdateThumbnail "+hr);
   if(controlled){
    pad=CreateWindowExW(0x08080080,"OrbitDesktopObserver","Controlled backdrop",0x40000000,0,0,w,h,GetParent(app),IntPtr.Zero,GetModuleHandleW(null),IntPtr.Zero);
    if(pad==IntPtr.Zero)throw new Exception("Backdrop creation failed");
    Backdrop(pad,pos,w,h,36,112,188);ShowWindow(pad,4);SetWindowPos(pad,app,0,0,0,0,0x13);
   }
   ShowWindow(app,0);Pump(700);Shot(f,w,h,prefix+"-background.png");
   ShowWindow(app,4);Pump(700);Shot(f,w,h,prefix+".png");
   if(controlled){
    DateTime before=File.GetLastWriteTimeUtc(a[7]);
    Backdrop(pad,pos,w,h,232,184,40);Pump(700);Shot(f,w,h,prefix+"-yellow.png");
    bool stable=before==File.GetLastWriteTimeUtc(a[7]);
    File.WriteAllText(prefix+"-source-stability.json","{\"foregroundNotRedrawnBetweenBlueAndYellow\":"+(stable?"true":"false")+"}");
   }
   Console.WriteLine("PASS embedded DWM observer");
  }finally{if(visible)ShowWindow(app,4);else ShowWindow(app,0);if(thumb!=IntPtr.Zero)DwmUnregisterThumbnail(thumb);if(pad!=IntPtr.Zero)DestroyWindow(pad);DestroyWindow(f);}
 }
}
