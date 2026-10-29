#!/usr/bin/env bash
# No SKIP: --adapter fallback is required from phase 6.
# Top-down at azimuth 0 the eye is north of the target, so the top of the image is south and the bottom is north.
# The bar is relative, not a fixed ratio: a low winter sun lights the ground weakly, so lit vs shadow is close.
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cam='orbit,azimuth=0,elevation=89,distance=40,target=5,4,0'
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roomdetect {}' --save "$tmp/p.dxf" >/dev/null 2>&1 || { echo "FAIL: run failed"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/w.png" --size 480x320 --camera "$cam" --ground \
  --sun 'lat=51.05,date=2026-12-21,time=solar-noon' --adapter fallback 2>"$tmp/e.log"
[ $? -eq 0 ] || { echo "FAIL: render --adapter fallback"; head -5 "$tmp/e.log"; exit 1; }
grep -q 'SHADOW' "$tmp/e.log" || { echo "FAIL: no shadow map reported"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/s.png" --size 480x320 --camera "$cam" --ground \
  --sun 'lat=51.05,date=2026-06-21,time=solar-noon' --adapter fallback >/dev/null 2>&1 \
  || { echo "FAIL: render (summer sun)"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/n.png" --size 480x320 --camera "$cam" \
  --adapter fallback >/dev/null 2>&1 || { echo "FAIL: render without --ground"; exit 1; }
python3 - "$tmp/w.png" "$tmp/s.png" "$tmp/n.png" <<'PY' || exit 1
import struct,sys,zlib
def decode(path):
    b=open(path,'rb').read(); assert b[:8]==b'\x89PNG\r\n\x1a\n'
    i=8; w=h=0; idat=b''; ct=6
    while i<len(b):
        ln,typ=struct.unpack_from('>I4s',b,i); body=b[i+8:i+8+ln]; i+=12+ln
        if typ==b'IHDR': w,h,_,ct=struct.unpack('>IIBB',body[:10])
        elif typ==b'IDAT': idat+=body
        elif typ==b'IEND': break
    nch=3 if ct==2 else 4; stride=w*nch; raw=zlib.decompress(idat); q=0; prev=bytearray(stride); rows=[]
    for _ in range(h):
        f=raw[q]; q+=1; line=bytearray(raw[q:q+stride]); q+=stride
        if f==1:
            for j in range(nch,stride): line[j]=(line[j]+line[j-nch])&255
        elif f==2:
            for j in range(stride): line[j]=(line[j]+prev[j])&255
        elif f==3:
            for j in range(stride): line[j]=(line[j]+((line[j-nch] if j>=nch else 0)+prev[j])//2)&255
        elif f==4:
            for j in range(stride):
                a=line[j-nch] if j>=nch else 0; bb=prev[j]; c=prev[j-nch] if j>=nch else 0
                pp=a+bb-c; pa=abs(pp-a); pb=abs(pp-bb); pc=abs(pp-c)
                line[j]=(line[j]+(a if pa<=pb and pa<=pc else bb if pb<=pc else c))&255
        rows.append(bytes(line)); prev=line
    return w,h,nch,rows
_,h,nch,winter=decode(sys.argv[1]); _,_,_,summer=decode(sys.argv[2]); _,_,_,bare=decode(sys.argv[3])
X=range(200,280)   # a column band through the middle of the house
def row(img,y):
    v=[]
    for x in X:
        c=img[y][x*nch:x*nch+nch]; n=bare[y][x*nch:x*nch+nch]
        assert max(abs(c[k]-n[k]) for k in range(3))>8, f"({x},{y}) is the same with and without --ground - ground missing?"
        v.append(sum(c[:3])/3)
    return sum(v)/len(v)
def band(img,y0,y1): return sum(row(img,y) for y in range(y0,y1))/(y1-y0)
south=band(winter,100,115); north=band(winter,205,220); far=band(winter,300,315)
assert south>20, f"lit ground is black ({south:.1f}) - no sun light"
assert north<south-15, f"north {north:.1f} not darker than south {south:.1f} - no shadow"
assert far>north+15, f"far north {far:.1f} as dark as just north {north:.1f} - darkening is not a shadow of the house"
def length(img):   # shadowed rows north of the house: darker than the lit far ground by half the shadow's depth
    lit=band(img,300,315); dark=band(winter,205,220); cut=(lit+dark)/2
    return sum(1 for y in range(203,300) if row(img,y)<cut)
lw=length(winter); ls=length(summer)
assert ls>0, "no shadow north of the house under the summer sun"
r=lw/ls   # tan(62.39)/tan(15.51) = 6.9 for a 2.4 m wall; counted from just past the wall tops, so about 9 here
assert 4<=r<=15, f"winter shadow {lw} px vs summer {ls} px (ratio {r:.1f}) - expected 4 to 15 (6.9 in theory): the shadow does not follow the sun's elevation"
print(f"ok: south {south:.1f}, north {north:.1f}, far north {far:.1f}; shadow {lw} px winter vs {ls} px summer (ratio {r:.1f})")
PY
echo "PASS 03"
