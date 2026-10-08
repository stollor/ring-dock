from PIL import Image
from pathlib import Path
import struct,json
out=Path('target/orbit-test')
for p in out.glob('*.bgra'):
 b=p.read_bytes();w,h=struct.unpack('<ii',b[:8]); pixels=bytearray(b[8:]); semi=0;bad=0
 for i in range(0,len(pixels),4):
  a=pixels[i+3]
  bad+=int(any(v>a for v in pixels[i:i+3]));semi+=int(0<a<255)
  if a:
   for j in range(3):pixels[i+j]=min(255,round(pixels[i+j]*255/a))
 im=Image.frombytes('RGBA',(w,h),bytes(pixels),'raw','BGRA');im.save(p.with_suffix('.png'))
 bg=Image.new('RGBA',im.size,(60,72,90,255));bg.alpha_composite(im);m=json.loads(p.with_suffix('.bgra.json').read_text(encoding='utf-8-sig'));cx,cy=m['center'];panel=m.get('panel');bottom=panel['y']+panel['h']+20 if panel else cy+195
 bg.crop((int(cx-265),int(cy-195),int(cx+265),int(bottom))).save(out/(p.stem+'-preview.png'))
 print(p.name,'semi',semi,'invalid premultiplied',bad)
