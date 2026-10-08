// Standalone diagnostic: same source compiled with/without supportedOS manifest.
// No injected clicks/keys, no desktop toggling, no production config changes.
using System;
using System.IO;
using System.Text;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Collections.Generic;
using System.Web.Script.Serialization;
public class TransparencyProbe {
  delegate IntPtr Proc(IntPtr h,uint m,IntPtr w,IntPtr l);
  delegate bool EnumProc(IntPtr h,IntPtr l);
  [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct WC {public uint style;public Proc proc;public int a,b;public IntPtr inst,icon,cursor,brush;public string menu,name;}
  [StructLayout(LayoutKind.Sequential)] struct PT {public int x,y;public PT(int a,int b){x=a;y=b;}}
  [StructLayout(LayoutKind.Sequential)] struct SZ {public int x,y;}
  [StructLayout(LayoutKind.Sequential)] struct RC {public int l,t,r,b;public RC(int x,int y,int w,int h){l=x;t=y;r=x+w;b=y+h;}}
  [StructLayout(LayoutKind.Sequential)] struct BF {public byte op,flags,alpha,format;}
  [StructLayout(LayoutKind.Sequential)] struct BI {public uint size;public int w,h;public ushort planes,bits;public uint compression,image;public int xppm,yppm;public uint used,important,color;}
  [StructLayout(LayoutKind.Sequential)] struct MSG {public IntPtr h;public uint m;public UIntPtr w;public IntPtr l;public uint time;public PT pt;public uint priv;}
  [StructLayout(LayoutKind.Sequential)] struct THUMB {public uint flags;public RC dest,source;public byte opacity;[MarshalAs(UnmanagedType.Bool)]public bool visible;[MarshalAs(UnmanagedType.Bool)]public bool client;}
  [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern ushort RegisterClassW(ref WC c);
  [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern IntPtr CreateWindowExW(uint ex,string cls,string title,uint style,int x,int y,int w,int h,IntPtr parent,IntPtr menu,IntPtr inst,IntPtr data);
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode)]static extern IntPtr GetModuleHandleW(string n);
  [DllImport("user32.dll")]static extern IntPtr DefWindowProcW(IntPtr h,uint m,IntPtr w,IntPtr l);
  [DllImport("user32.dll")]static extern bool DestroyWindow(IntPtr h);
  [DllImport("user32.dll")]static extern bool ShowWindow(IntPtr h,int n);
  [DllImport("user32.dll")]static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int ht,uint flags);
  [DllImport("user32.dll")]static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")]static extern int ReleaseDC(IntPtr h,IntPtr dc);
  [DllImport("user32.dll")]static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll",SetLastError=true)]static extern IntPtr SetParent(IntPtr h,IntPtr p);
  [DllImport("user32.dll")]static extern int GetWindowLongW(IntPtr h,int i);
  [DllImport("user32.dll")]static extern int SetWindowLongW(IntPtr h,int i,int v);
  [DllImport("user32.dll")]static extern bool ClientToScreen(IntPtr h,ref PT p);
  [DllImport("user32.dll")]static extern bool ScreenToClient(IntPtr h,ref PT p);
  [DllImport("user32.dll")]static extern bool GetWindowRect(IntPtr h,out RC r);
  [DllImport("user32.dll")]static extern IntPtr WindowFromPoint(PT p);
  [DllImport("user32.dll")]static extern bool PeekMessageW(out MSG m,IntPtr h,uint min,uint max,uint remove);
  [DllImport("user32.dll")]static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")]static extern IntPtr DispatchMessageW(ref MSG m);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")]static extern IntPtr GetShellWindow();
  [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc cb,IntPtr l);
  [DllImport("user32.dll")]static extern bool EnumChildWindows(IntPtr h,EnumProc cb,IntPtr l);
  [DllImport("user32.dll",SetLastError=true)]static extern bool UpdateLayeredWindow(IntPtr h,IntPtr dst,ref PT pos,ref SZ size,IntPtr src,ref PT from,uint key,ref BF blend,uint flags);
  [DllImport("gdi32.dll")]static extern IntPtr CreateCompatibleDC(IntPtr h);
  [DllImport("gdi32.dll")]static extern bool DeleteDC(IntPtr h);
  [DllImport("gdi32.dll")]static extern bool DeleteObject(IntPtr h);
  [DllImport("gdi32.dll")]static extern IntPtr SelectObject(IntPtr h,IntPtr o);
  [DllImport("gdi32.dll")]static extern IntPtr CreateDIBSection(IntPtr dc,ref BI bi,uint usage,out IntPtr bits,IntPtr section,uint off);
  [DllImport("gdi32.dll")]static extern uint GetPixel(IntPtr dc,int x,int y);
  [DllImport("dwmapi.dll")]static extern int DwmRegisterThumbnail(IntPtr dest,IntPtr src,out IntPtr thumb);
  [DllImport("dwmapi.dll")]static extern int DwmUpdateThumbnailProperties(IntPtr thumb,ref THUMB props);
  [DllImport("dwmapi.dll")]static extern int DwmUnregisterThumbnail(IntPtr thumb);
  [DllImport("user32.dll")]static extern bool SetProcessDPIAware();
  static Proc callback=WndProc;
  static List<IntPtr> windows=new List<IntPtr>();
  static Dictionary<IntPtr,int> savedStyles=new Dictionary<IntPtr,int>();
  static IntPtr thumbnail=IntPtr.Zero;
  static string outDir,label;
  static readonly int[] alphas={0,1,64,128,192,255};
  static Color bg=Color.FromArgb(36,112,188);
  static readonly Color fg=Color.FromArgb(224,48,80);
  static IntPtr WndProc(IntPtr h,uint m,IntPtr w,IntPtr l){return DefWindowProcW(h,m,w,l);}
  static void Pump(int ms){DateTime end=DateTime.Now.AddMilliseconds(ms);MSG m;while(DateTime.Now<end){while(PeekMessageW(out m,IntPtr.Zero,0,0,1)){TranslateMessage(ref m);DispatchMessageW(ref m);}System.Threading.Thread.Sleep(5);}}
  static string ClassOf(IntPtr h){StringBuilder b=new StringBuilder(128);GetClassNameW(h,b,128);return b.ToString();}
  static IntPtr Create(uint ex,uint style,IntPtr parent,int x,int y,int w,int h){IntPtr p=CreateWindowExW(ex,"RingTransparencyProbe",label,style,x,y,w,h,parent,IntPtr.Zero,GetModuleHandleW(null),IntPtr.Zero);if(p!=IntPtr.Zero)windows.Add(p);return p;}
  static void Paint(IntPtr h,Color color){IntPtr dc=GetDC(h);using(Graphics g=Graphics.FromHdc(dc)){g.Clear(color);}ReleaseDC(h,dc);}
  static bool Present(IntPtr h,int x,int y,out int err){return PresentPixels(h,x,y,false,out err);}
  static bool PresentPixels(IntPtr h,int x,int y,bool solid,out int err){int w=360,ht=120;byte[] data=new byte[w*ht*4];for(int yy=0;yy<ht;yy++)for(int xx=0;xx<w;xx++){int a=solid?255:alphas[xx/60],i=(yy*w+xx)*4;Color c=solid?bg:fg;data[i]=(byte)((c.B*a+127)/255);data[i+1]=(byte)((c.G*a+127)/255);data[i+2]=(byte)((c.R*a+127)/255);data[i+3]=(byte)a;}
    return PresentRaw(h,x,y,w,ht,data,out err);
  }
  static bool PresentRaw(IntPtr h,int x,int y,int w,int ht,byte[] data,out int err){
    IntPtr dc=GetDC(IntPtr.Zero),mem=CreateCompatibleDC(dc),bits;BI bi=new BI{size=40,w=w,h=-ht,planes=1,bits=32};IntPtr dib=CreateDIBSection(mem,ref bi,0,out bits,IntPtr.Zero,0);if(dib==IntPtr.Zero){err=Marshal.GetLastWin32Error();DeleteDC(mem);ReleaseDC(IntPtr.Zero,dc);return false;}IntPtr old=SelectObject(mem,dib);Marshal.Copy(data,0,bits,data.Length);PT p=new PT(x,y),src=new PT();SZ sz=new SZ{x=w,y=ht};BF blend=new BF{alpha=255,format=1};bool ok=UpdateLayeredWindow(h,dc,ref p,ref sz,mem,ref src,0,ref blend,2);err=ok?0:Marshal.GetLastWin32Error();SelectObject(mem,old);DeleteObject(dib);DeleteDC(mem);ReleaseDC(IntPtr.Zero,dc);return ok;
  }
  static int[] RGB(uint c){return new int[]{(int)(c&255),(int)((c>>8)&255),(int)((c>>16)&255)};}
  static object Sample(int x,int y,int a,IntPtr h,bool hit){IntPtr dc=GetDC(IntPtr.Zero);int[] actual=RGB(GetPixel(dc,x,y));ReleaseDC(IntPtr.Zero,dc);int[] expected={ (fg.R*a+bg.R*(255-a)+127)/255,(fg.G*a+bg.G*(255-a)+127)/255,(fg.B*a+bg.B*(255-a)+127)/255};int e=Math.Max(Math.Abs(actual[0]-expected[0]),Math.Max(Math.Abs(actual[1]-expected[1]),Math.Abs(actual[2]-expected[2])));return new{alpha=a,actual=actual,expected=expected,maxError=e,pixelPass=e<=2,hitWindow=hit?ClassOf(WindowFromPoint(new PT(x,y))):"not-tested-occluded-desktop",hitIsProbe=hit?(object)(WindowFromPoint(new PT(x,y))==h):null};}
  static void Shot(string name,int x,int y,int w,int h){using(Bitmap b=new Bitmap(w,h)){using(Graphics g=Graphics.FromImage(b)){g.CopyFromScreen(x,y,0,0,new Size(w,h));}b.Save(Path.Combine(outDir,label+"-"+name+".png"),ImageFormat.Png);}}
  static IntPtr FindClass(string name){IntPtr found=IntPtr.Zero;EnumProc cb=delegate(IntPtr h,IntPtr l){if(ClassOf(h)==name){found=h;return false;}return true;};EnumProc top=delegate(IntPtr h,IntPtr l){if(ClassOf(h)==name){found=h;return false;}EnumChildWindows(h,cb,IntPtr.Zero);return found==IntPtr.Zero;};EnumWindows(top,IntPtr.Zero);return found;}
    static byte[] Prototype(){
    // Bitmap owns the premultiplied alpha. No screenshot pixels enter this frame.
    using(Bitmap b=new Bitmap(360,320,PixelFormat.Format32bppPArgb)){
      using(Graphics g=Graphics.FromImage(b)){
        g.Clear(Color.Transparent);g.SmoothingMode=System.Drawing.Drawing2D.SmoothingMode.AntiAlias;g.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
        using(var path=new System.Drawing.Drawing2D.GraphicsPath(System.Drawing.Drawing2D.FillMode.Alternate))using(var fill=new SolidBrush(Color.FromArgb(185,23,25,33))){path.AddEllipse(35,15,290,290);path.AddEllipse(85,65,190,190);g.FillPath(fill,path);}
        using(var pen=new Pen(Color.FromArgb(235,106,174,242),2.5f)){for(int i=0;i<4;i++)g.DrawArc(pen,62,42,236,236,i*90+6,76);}
        using(var pen=new Pen(Color.FromArgb(255,230,236,246),1.5f)){g.DrawRectangle(pen,249,61,30,22);g.DrawEllipse(pen,79,59,19,19);g.DrawRectangle(pen,83,235,23,15);g.DrawRectangle(pen,259,233,16,22);}
        using(var path=new System.Drawing.Drawing2D.GraphicsPath())using(var fill=new SolidBrush(Color.FromArgb(160,23,25,33))){path.AddArc(112,119,16,16,180,90);path.AddArc(232,119,16,16,270,90);path.AddArc(232,188,16,16,0,90);path.AddArc(112,188,16,16,90,90);path.CloseFigure();g.FillPath(fill,path);}
        using(var font=new Font("Segoe UI Semibold",30,FontStyle.Regular,GraphicsUnit.Pixel))using(var brush=new SolidBrush(Color.White))using(var fmt=new StringFormat{Alignment=StringAlignment.Center,LineAlignment=StringAlignment.Center}){g.DrawString("17:29",font,brush,new RectangleF(112,126,136,43),fmt);}
        using(var font=new Font("Segoe UI",12,FontStyle.Regular,GraphicsUnit.Pixel))using(var brush=new SolidBrush(Color.FromArgb(255,223,230,240)))using(var fmt=new StringFormat{Alignment=StringAlignment.Center,LineAlignment=StringAlignment.Center}){g.DrawString("10月3日  周六",font,brush,new RectangleF(112,169,136,25),fmt);}
      }
      var locked=b.LockBits(new Rectangle(0,0,360,320),ImageLockMode.ReadOnly,PixelFormat.Format32bppPArgb);byte[] bytes=new byte[360*320*4];try{for(int y=0;y<320;y++)Marshal.Copy(IntPtr.Add(locked.Scan0,y*locked.Stride),bytes,y*360*4,360*4);}finally{b.UnlockBits(locked);}return bytes;
    }
  }
  static List<IntPtr> Hosts(){List<IntPtr> list=new List<IntPtr>();EnumProc scan=delegate(IntPtr h,IntPtr l){string c=ClassOf(h);if(c=="SysListView32"||c=="SHELLDLL_DefView")list.Add(h);return true;};EnumProc tops=delegate(IntPtr h,IntPtr l){if(ClassOf(h)=="Progman"||ClassOf(h)=="WorkerW")EnumChildWindows(h,scan,IntPtr.Zero);return true;};EnumWindows(tops,IntPtr.Zero);IntPtr shell=GetShellWindow();if(shell!=IntPtr.Zero)list.Add(shell);return list;}
  static void ClipAncestors(IntPtr h){for(IntPtr p=h;p!=IntPtr.Zero;p=GetParent(p)){if(!savedStyles.ContainsKey(p)){int s=GetWindowLongW(p,-16);savedStyles.Add(p,s);SetWindowLongW(p,-16,s|0x02000000);}}}
  static void Main(string[] args){SetProcessDPIAware();outDir=args[0];label=args[1];Directory.CreateDirectory(outDir);WC wc=new WC{proc=callback,inst=GetModuleHandleW(null),name="RingTransparencyProbe"};RegisterClassW(ref wc);List<object> rows=new List<object>();
    try {
      IntPtr display=Create(0x08000088,0x80000000,IntPtr.Zero,120,140,420,210);ShowWindow(display,8);SetWindowPos(display,new IntPtr(-1),120,140,420,210,0x50);Paint(display,bg);Pump(300);
      // Direct WS_CHILD creation checks the compatibility gate independently of SetParent.
      IntPtr direct=Create(0x00080000,0x40000000,display,20,40,360,120);int createErr=direct==IntPtr.Zero?Marshal.GetLastWin32Error():0;
      int err=0;bool ok=direct!=IntPtr.Zero&&Present(direct,20,40,out err);if(direct!=IntPtr.Zero)ShowWindow(direct,8);Pump(300);PT org=new PT(20,40);ClientToScreen(display,ref org);List<object> samples=new List<object>();for(int i=0;i<6;i++)samples.Add(Sample(org.x+30+i*60,org.y+60,alphas[i],direct,true));rows.Add(new{mode="own-parent-direct-child",created=direct!=IntPtr.Zero,createError=createErr,present=ok,presentError=err,samples=samples});Shot("own-child",120,140,420,210);if(direct!=IntPtr.Zero){DestroyWindow(direct);windows.Remove(direct);}Paint(display,bg);
      // Match production: create popup, switch style, SetParent; then read back.
      IntPtr converted=Create(0x00080000,0x80000000,IntPtr.Zero,0,0,360,120);SetWindowLongW(converted,-16,0x40000000);SetParent(converted,display);bool parentOK=GetParent(converted)==display;ok=Present(converted,20,40,out err);ShowWindow(converted,8);Pump(300);samples=new List<object>();for(int i=0;i<6;i++)samples.Add(Sample(org.x+30+i*60,org.y+60,alphas[i],converted,true));rows.Add(new{mode="own-parent-popup-setparent",parentOK=parentOK,present=ok,presentError=err,samples=samples});Shot("converted-child",120,140,420,210);DestroyWindow(converted);windows.Remove(converted);
      foreach(IntPtr host in Hosts()){
        ClipAncestors(host);IntPtr root=host;while(GetParent(root)!=IntPtr.Zero)root=GetParent(root);RC rootRect;GetWindowRect(root,out rootRect);
        // Opaque layered sibling provides known live background. DWM thumbnail captures the
        // real desktop subtree without minimizing any user application.
        IntPtr pad=Create(0x00080000,0x80000000,IntPtr.Zero,0,0,360,120);SetWindowLongW(pad,-16,0x40000000);SetParent(pad,host);int padErr;bool padPresented=PresentPixels(pad,520,320,true,out padErr);ShowWindow(pad,8);SetWindowPos(pad,IntPtr.Zero,0,0,0,0,0x13);PT p0=new PT(500,300);ClientToScreen(host,ref p0);
        int hr=DwmRegisterThumbnail(display,root,out thumbnail);THUMB t=new THUMB{flags=0x1|0x2|0x4|0x8|0x10,dest=new RC(10,20,400,160),source=new RC(p0.x-rootRect.l,p0.y-rootRect.t,400,160),opacity=255,visible=true,client=false};int update=hr==0?DwmUpdateThumbnailProperties(thumbnail,ref t):hr;Pump(400);PT dest=new PT(10,20);ClientToScreen(display,ref dest);IntPtr dc=GetDC(IntPtr.Zero);int[] baseline=RGB(GetPixel(dc,dest.x+200,dest.y+80));ReleaseDC(IntPtr.Zero,dc);bool captureOK=Math.Abs(baseline[0]-bg.R)<=2&&Math.Abs(baseline[1]-bg.G)<=2&&Math.Abs(baseline[2]-bg.B)<=2;
        IntPtr layer=Create(0x00080000,0x80000000,IntPtr.Zero,0,0,360,120);SetWindowLongW(layer,-16,0x40000000);SetParent(layer,host);parentOK=GetParent(layer)==host;ok=Present(layer,520,320,out err);ShowWindow(layer,8);SetWindowPos(layer,IntPtr.Zero,0,0,0,0,0x13);Pump(500);samples=new List<object>();for(int i=0;i<6;i++)samples.Add(Sample(dest.x+20+30+i*60,dest.y+20+60,alphas[i],layer,false));rows.Add(new{mode="desktop-"+ClassOf(host),root=ClassOf(root),parentOK=parentOK,present=ok,presentError=err,thumbnailHR=hr,thumbnailUpdateHR=update,captureControlPass=captureOK,padPresented=padPresented,padError=padErr,baseline=baseline,samples=samples});Shot("desktop-"+ClassOf(host),120,140,420,210);
        if(captureOK){bg=Color.FromArgb(232,184,40);PresentPixels(pad,520,320,true,out padErr);Pump(350);List<object> changed=new List<object>();for(int i=0;i<6;i++)changed.Add(Sample(dest.x+50+i*60,dest.y+80,alphas[i],layer,false));rows.Add(new{mode="desktop-live-background-change-"+ClassOf(host),backgroundRGB=new int[]{bg.R,bg.G,bg.B},foregroundRepainted=false,samples=changed});Shot("desktop-live-"+ClassOf(host),120,140,420,210);bg=Color.FromArgb(36,112,188);}
        if(thumbnail!=IntPtr.Zero){DwmUnregisterThumbnail(thumbnail);thumbnail=IntPtr.Zero;}DestroyWindow(layer);windows.Remove(layer);DestroyWindow(pad);windows.Remove(pad);Paint(display,bg);
      }
      IntPtr current=FindClass("RingDockVisual");if(current!=IntPtr.Zero){IntPtr root=current;while(GetParent(root)!=IntPtr.Zero)root=GetParent(root);RC appRect,rootRect;GetWindowRect(current,out appRect);GetWindowRect(root,out rootRect);SetWindowPos(display,new IntPtr(-1),120,140,420,350,0x50);int hr=DwmRegisterThumbnail(display,root,out thumbnail);THUMB t=new THUMB{flags=31,dest=new RC(10,10,400,330),source=new RC((appRect.l+appRect.r)/2-200-rootRect.l,(appRect.t+appRect.b)/2-165-rootRect.t,400,330),opacity=255,visible=true,client=false};if(hr==0)DwmUpdateThumbnailProperties(thumbnail,ref t);Pump(500);Shot("current-ring",120,140,420,350);rows.Add(new{mode="current-ring-capture",thumbnailHR=hr});if(thumbnail!=IntPtr.Zero){DwmUnregisterThumbnail(thumbnail);thumbnail=IntPtr.Zero;}}
            IntPtr protoHost=Hosts().Find(h => ClassOf(h)=="SysListView32");if(protoHost!=IntPtr.Zero){
        ClipAncestors(protoHost);IntPtr root=protoHost;while(GetParent(root)!=IntPtr.Zero)root=GetParent(root);RC rootRect;GetWindowRect(root,out rootRect);
        IntPtr proto=Create(0x00080000,0x80000000,IntPtr.Zero,0,0,360,320);SetWindowLongW(proto,-16,0x40000000);SetParent(proto,protoHost);ok=PresentRaw(proto,500,300,360,320,Prototype(),out err);ShowWindow(proto,8);SetWindowPos(proto,IntPtr.Zero,0,0,0,0,0x13);
        SetWindowPos(display,new IntPtr(-1),120,140,420,350,0x50);PT origin=new PT(480,295);ClientToScreen(protoHost,ref origin);int hr=DwmRegisterThumbnail(display,root,out thumbnail);THUMB props=new THUMB{flags=31,dest=new RC(10,10,400,330),source=new RC(origin.x-rootRect.l,origin.y-rootRect.t,400,330),opacity=255,visible=true,client=false};if(hr==0)DwmUpdateThumbnailProperties(thumbnail,ref props);Pump(500);Shot("prototype",120,140,420,350);rows.Add(new{mode="desktop-ring-prototype",parentOK=GetParent(proto)==protoHost,present=ok,presentError=err,thumbnailHR=hr,ringAlpha=185,clockBackgroundAlpha=160,textAlpha=255});if(thumbnail!=IntPtr.Zero){DwmUnregisterThumbnail(thumbnail);thumbnail=IntPtr.Zero;}DestroyWindow(proto);windows.Remove(proto);
      }
      string json=new JavaScriptSerializer().Serialize(new{label=label,os=Environment.OSVersion.ToString(),sourceRGB=new int[]{fg.R,fg.G,fg.B},backgroundRGB=new int[]{bg.R,bg.G,bg.B},rows=rows});File.WriteAllText(Path.Combine(outDir,label+".json"),json);Console.WriteLine(json);
    } finally {if(thumbnail!=IntPtr.Zero)DwmUnregisterThumbnail(thumbnail);for(int i=windows.Count-1;i>=0;i--)DestroyWindow(windows[i]);foreach(var kv in savedStyles){int current=GetWindowLongW(kv.Key,-16);SetWindowLongW(kv.Key,-16,(current&~0x02000000)|(kv.Value&0x02000000));}}
  }
}
