from PIL import Image
from pathlib import Path
import struct,json,statistics
out=Path('reports/orbit-glass');rows=[]
for name in ['desktop-closed','desktop-expanded','desktop-editing']:
 r=json.loads((out/(name+'-roi.json')).read_text(encoding='utf-8-sig'));b=Path('target/orbit-desktop-test/'+name+'.bgra').read_bytes();w,h=struct.unpack('<ii',b[:8]);raw=Image.frombytes('RGBA',(w,h),b[8:],'raw','BGRA').crop((r['x'],r['y'],r['x']+r['width'],r['y']+r['height']))
 bg=Image.open(out/(name+'-background.png')).convert('RGB');im=Image.open(out/(name+'.png')).convert('RGB');semi=[];transparent=[];solid=[]
 for f,back,actual in zip(raw.getdata(),bg.getdata(),im.getdata()):
  a=f[3];expected=[round(f[i]+back[i]*(1-a/255)) for i in range(3)];e=max(abs(actual[i]-expected[i]) for i in range(3))
  if 32<a<224:semi.append(e)
  elif a==0:transparent.append(e)
  elif a==255:solid.append(e)
 rows.append(dict(name=name,semiSamples=len(semi),semiWithin2=sum(e<=2 for e in semi),semiMaxError=max(semi),semiMedian=statistics.median(semi),backgroundStableWithin2=sum(e<=2 for e in transparent),transparentSamples=len(transparent),opaqueMedian=statistics.median(solid),premultipliedValid=all(max(p[:3])<=p[3] for p in raw.getdata())))
print(json.dumps(rows,indent=2));(out/'compositing-results.json').write_text(json.dumps(rows,indent=2),encoding='utf-8')
