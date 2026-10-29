#!/usr/bin/env bash
# No SKIP: --adapter fallback is required from phase 6.
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roomdetect {}' --save "$tmp/p.dxf" >/dev/null 2>&1 || { echo "FAIL: run failed"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/s.png" --size 480x320 \
  --camera 'orbit,azimuth=0,elevation=89,distance=40,target=5,4,0' --ground \
  --sun 'lat=51.05,date=2026-12-21,time=solar-noon' --adapter fallback 2>"$tmp/e.log"
[ $? -eq 0 ] || { echo "FAIL: render --adapter fallback"; head -5 "$tmp/e.log"; exit 1; }
grep -q 'SHADOW' "$tmp/e.log" || { echo "FAIL: no shadow map reported"; exit 1; }
python3 - "$tmp/s.png" <<'PY' || exit 1
import struct,sys,zlib
b=open(sys.argv[1],'rb').read(); assert b[:8]==b'\x89PNG\r\n\x1a\n'
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
bg=rows[h-1][:nch]
def band(y0,y1):
    v=[]
    for y in range(y0,y1):
        for x in range(200,280):
            c=rows[y][x*nch:x*nch+nch]
            assert max(abs(c[k]-bg[k]) for k in range(3))>8, f"({x},{y}) is background - ground missing?"
            v.append(sum(c[:3])/3)
    return sum(v)/len(v)
south=band(100,115); north=band(205,220)
assert south>20, f"lit ground is black ({south:.1f}) - no sun light"
assert north<0.75*south, f"north {north:.1f} not darker than south {south:.1f} - no shadow"
print(f"ok: shadow south {south:.1f} vs north {north:.1f}")
PY
echo "PASS 03"
