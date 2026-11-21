#!/usr/bin/env python3
"""Check all four rounded cutouts against a light native-page fixture."""
import argparse,json
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('image');a=p.parse_args()
im=Image.open(a.image).convert('RGB');w,h=im.size
light=lambda xy:min(im.getpixel(xy))>170
x=w//2
top=next(y for y in range(65,h//2) if light((x,y)))
bottom=max(y for y in range(top,h) if light((x,y)))
row=min(top+40,bottom)
left=next(i for i in range(w) if light((i,row)))
right=max(i for i in range(w) if light((i,row)))
assert right-left>w*.6 and bottom-top>h*.6,'native page extent missing'
points=[(left+2,top+2),(right-2,top+2),(left+2,bottom-2),(right-2,bottom-2)]
colors=[im.getpixel(point) for point in points]
passed=all(max(c)<140 for c in colors)
print(json.dumps({'image':a.image,'frame':[left,top,right,bottom],'corners':colors,'passed':passed}))
raise SystemExit(0 if passed else 1)
