#!/usr/bin/env python3
"""Generate VeilVPN launcher icon: dark glass rounded square + cyan veil arcs."""
from PIL import Image, ImageDraw
import os

SIZES = {'mdpi':48,'hdpi':72,'xhdpi':96,'xxhdpi':144,'xxxhdpi':192}
BASE = os.path.join(os.path.dirname(__file__), '..', 'android', 'res')

def icon(size):
    s = size * 4
    img = Image.new('RGBA', (s, s), (0,0,0,0))
    d = ImageDraw.Draw(img)
    r = int(s*0.22)
    d.rounded_rectangle([0,0,s-1,s-1], radius=r, fill=(11,8,32,255))
    d.ellipse([int(-s*0.15), int(-s*0.2), int(s*0.55), int(s*0.35)], fill=(124,58,237,110))
    d.ellipse([int(s*0.45), int(s*0.5), int(s*1.15), int(s*1.15)], fill=(6,182,212,110))
    d.ellipse([int(s*0.1), int(s*0.62), int(s*0.75), int(s*1.2)], fill=(236,72,153,80))
    cx, cy = s/2, s/2
    for k, w, col in [(0.36, s//20, (255,255,255,235)), (0.27, s//24, (125,211,252,220)), (0.18, s//28, (34,211,238,255))]:
        rad = s*k/2
        bbox = [cx-rad, cy-rad*1.05, cx+rad, cy+rad*1.05]
        d.arc(bbox, start=180, end=360, fill=col, width=w)
        d.polygon([(cx-rad, cy), (cx+rad, cy), (cx, cy+rad*1.12)], outline=col, width=w)
    return img.resize((size,size), Image.LANCZOS)

for q, px in SIZES.items():
    d = os.path.join(BASE, f'mipmap-{q}')
    os.makedirs(d, exist_ok=True)
    icon(px).save(os.path.join(d, 'ic_launcher.png'))
print('icons done')
