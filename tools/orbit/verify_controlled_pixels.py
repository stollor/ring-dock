from PIL import Image
from pathlib import Path
import struct,json,sys
out=Path(sys.argv[1]) if len(sys.argv)>1 else Path('reports/orbit-glass');r=json.loads((out/'desktop-alpha-roi.json').read_text(encoding='utf-8-sig'));b=Path('target/orbit-desktop-test/desktop-alpha.bgra').read_bytes();w,h=struct.unpack('<ii',b[:8]);raw=Image.frombytes('RGBA',(w,h),b[8:],'raw','BGRA').crop((r['x'],r['y'],r['x']+r['width'],r['y']+r['height']));rows=[]
control=Image.open(out/'desktop-alpha-background.png').convert('RGB');err=[max(abs(a[i]-[36,112,188][i]) for i in range(3)) for a in control.get_flattened_data()]
print('Known-background control max error',max(err),'within2',sum(e<=2 for e in err),'/',len(err));assert max(err)<=2,'Invalid observation baseline'
for name,color in [('desktop-alpha',[36,112,188]),('desktop-alpha-yellow',[232,184,40])]:
 im=Image.open(out/(name+'.png')).convert('RGB');semi=[];invalid=0
 for f,actual in zip(raw.get_flattened_data(),im.get_flattened_data()):
  a=f[3];invalid+=int(max(f[:3])>a)
  if 0<a<255:
   expected=[round(f[i]+color[i]*(1-a/255)) for i in range(3)];semi.append(max(abs(actual[i]-expected[i]) for i in range(3)))
 row=dict(name=name,semiSamples=len(semi),passed=sum(e<=2 for e in semi),maxError=max(semi),invalidPremultiplied=invalid);rows.append(row);print(row)
 assert max(semi)<=2 and invalid==0,'Alpha compositing mismatch'
(out/'controlled-alpha-results.json').write_text(json.dumps(dict(baselineMaxError=max(err),checks=rows),indent=2),encoding='utf-8')
