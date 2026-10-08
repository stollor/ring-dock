// Isolated reproduction of src/textrgn.rs's GDI DrawText/PathToRegion path.
// Does not modify any application window or inject input.
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Web.Script.Serialization;
public static class ClockPathProbe {
 [StructLayout(LayoutKind.Sequential)]struct R {public int l,t,r,b;}
 [DllImport("gdi32.dll")]static extern IntPtr CreateCompatibleDC(IntPtr h);
 [DllImport("gdi32.dll")]static extern bool DeleteDC(IntPtr h);
 [DllImport("gdi32.dll")]static extern IntPtr SelectObject(IntPtr h,IntPtr o);
 [DllImport("gdi32.dll")]static extern bool DeleteObject(IntPtr h);
 [DllImport("gdi32.dll",CharSet=CharSet.Unicode)]static extern IntPtr CreateFontW(int h,int w,int a,int b,int weight,uint italic,uint under,uint strike,uint charset,uint op,uint clip,uint quality,uint pitch,string name);
 [DllImport("gdi32.dll")]static extern int SetBkMode(IntPtr h,int mode);
 [DllImport("gdi32.dll")]static extern bool BeginPath(IntPtr h);
 [DllImport("gdi32.dll")]static extern bool EndPath(IntPtr h);
 [DllImport("gdi32.dll")]static extern IntPtr PathToRegion(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int DrawTextW(IntPtr h,string text,int len,ref R r,uint flags);
 [DllImport("gdi32.dll")]static extern bool PtInRegion(IntPtr h,int x,int y);
 public static object Run(string folder,bool transparent){
  const int w=220,ht=42;IntPtr dc=CreateCompatibleDC(IntPtr.Zero),font=CreateFontW(-34,0,0,0,300,0,0,0,0,0,0,0,0,"Segoe UI Light");IntPtr old=SelectObject(dc,font),rgn=IntPtr.Zero;
  try {SetBkMode(dc,transparent?1:2);R r=new R{r=w,b=ht};BeginPath(dc);DrawTextW(dc,"17:29",5,ref r,0x25|0x800);EndPath(dc);rgn=PathToRegion(dc);int n=0;
   using(Bitmap image=new Bitmap(w,ht)){for(int y=0;y<ht;y++)for(int x=0;x<w;x++){bool inside=PtInRegion(rgn,x,y);if(inside)n++;image.SetPixel(x,y,inside?Color.White:Color.FromArgb(23,25,33));}image.Save(System.IO.Path.Combine(folder,transparent?"clock-path-transparent.png":"clock-path-opaque.png"));}
   return new{backgroundMode=transparent?"TRANSPARENT":"OPAQUE (current default)",regionPixels=n,validRegion=rgn!=IntPtr.Zero};
  }finally{if(rgn!=IntPtr.Zero)DeleteObject(rgn);SelectObject(dc,old);DeleteObject(font);DeleteDC(dc);}
 }
 public static void Main(string[] args){string json=new JavaScriptSerializer().Serialize(new object[]{Run(args[0],false),Run(args[0],true)});System.IO.File.WriteAllText(System.IO.Path.Combine(args[0],"clock-path.json"),json);Console.WriteLine(json);}
}
