param([string]$Path, [int]$X, [int]$Y, [int]$HoverX=$X, [int]$HoverY=$Y, [int]$Effects=1, [switch]$TextData)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out=Join-Path $root 'target\orbit-drag-test'
$source=Join-Path $out 'drag-source.cs';$exe=Join-Path $out 'drag-source.exe'
@"
using System;
using System.Windows.Forms;
using System.Runtime.InteropServices;
using System.Drawing;
public class DragForm : Form {
 protected override CreateParams CreateParams { get { var p=base.CreateParams;p.ExStyle |= 0x8;return p; } }
 protected override bool ShowWithoutActivation { get { return true; } }
}
public class Entry {
 [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h,int n);
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(POINT p);
 [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] static extern bool GetCursorPos(out POINT p);
 [DllImport("user32.dll")] static extern void mouse_event(uint f,uint x,uint y,uint data,UIntPtr extra);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr p);
 struct POINT {public int x,y;}
 [STAThread] public static void Main(string[] args) {
  SetThreadDpiAwarenessContext(new IntPtr(-4));
  string path=args[0];int x=int.Parse(args[1]),y=int.Parse(args[2]),hx=int.Parse(args[3]),hy=int.Parse(args[4]);
  POINT old;GetCursorPos(out old);
  Rectangle work=Screen.FromPoint(new Point(hx,hy)).WorkingArea;
  Form f=new DragForm();f.FormBorderStyle=FormBorderStyle.None;f.ShowInTaskbar=false;f.StartPosition=FormStartPosition.Manual;f.Bounds=new Rectangle(work.Left+20,work.Top+20,80,40);f.BackColor=Color.FromArgb(30,40,50);
  try {
   f.Show();ShowWindow(f.Handle,4);Application.DoEvents();SetCursorPos(f.Left+20,f.Top+20);System.Threading.Thread.Sleep(100);
   POINT start=new POINT{x=f.Left+20,y=f.Top+20};if(WindowFromPoint(start)!=f.Handle)throw new Exception("Test source window not visible; refusing mouse down");
   mouse_event(2,0,0,0,UIntPtr.Zero);System.Threading.Thread.Sleep(100);Application.DoEvents();
   var t=new System.Threading.Thread(()=>{
    SetThreadDpiAwarenessContext(new IntPtr(-4));
    for(int i=0;i<20;i++) {SetCursorPos((i<9?hx:x)+(i%3),(i<9?hy:y)+(i%2));System.Threading.Thread.Sleep(100);}
    mouse_event(4,0,0,0,UIntPtr.Zero);
   });t.IsBackground=true;t.Start();
   DataObject data=new DataObject();if(args[6]=="text")data.SetText("unsupported text drop");else data.SetData(DataFormats.FileDrop,new string[]{path});
   var effect=f.DoDragDrop(data,(DragDropEffects)int.Parse(args[5]));t.Join();Console.WriteLine("DoDragDrop effect="+(int)effect);
  } finally { mouse_event(4,0,0,0,UIntPtr.Zero);SetCursorPos(old.x,old.y);f.Close();f.Dispose(); }
 }
}
"@ | Set-Content $source -Encoding utf8
& (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe') /nologo /target:exe /platform:x64 /r:System.Windows.Forms.dll /r:System.Drawing.dll "/out:$exe" $source
if($LASTEXITCODE -ne 0){throw 'Drag source compilation failed'}
$p=Start-Process $exe -ArgumentList @(('"'+$Path+'"'),$X,$Y,$HoverX,$HoverY,$Effects,($(if($TextData){'text'}else{'file'}))) -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $out 'source-output.txt') -RedirectStandardError (Join-Path $out 'source-error.txt')
if(-not $p.WaitForExit(10000)){Stop-Process -Id $p.Id;throw 'Drag source timeout'}
if($p.ExitCode -ne 0){throw 'Drag source failed'}
Get-Content (Join-Path $out 'source-output.txt')
