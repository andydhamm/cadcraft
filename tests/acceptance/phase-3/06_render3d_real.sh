#!/usr/bin/env bash
# render3d really draws: depth-tested, lit image. SKIP only on exit 3 (NO_ADAPTER).
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' --cmd 'roomdetect {}' \
  --save "$tmp/p.dxf" >/dev/null 2>&1 || { echo "FAIL: run failed"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/a.png" --size 480x320 --camera 'orbit,azimuth=35,elevation=30,distance=25' 2>"$tmp/e1.log"
rc=$?
if [ "$rc" -eq 3 ]; then grep -q NO_ADAPTER "$tmp/e1.log" || { echo "FAIL: exit 3 without NO_ADAPTER"; head -5 "$tmp/e1.log"; exit 1; }; echo "SKIP 06: no GPU adapter"; exit 0; fi
[ "$rc" -eq 0 ] || { echo "FAIL: render exit $rc"; head -5 "$tmp/e1.log"; exit 1; }
[ -s "$tmp/a.png" ] || { echo "FAIL: no png written"; exit 1; }
grep -q '^PROJECTED ' "$tmp/e1.log" || { echo "FAIL: render must print 'PROJECTED x0,y0,x1,y1' in pixels"; head -5 "$tmp/e1.log"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/b.png" --size 480x320 --camera 'orbit,azimuth=215,elevation=30,distance=25' 2>/dev/null || { echo "FAIL: second camera render"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/w.png" --size 480x320 --camera 'orbit,azimuth=35,elevation=89,distance=25' 2>/dev/null || { echo "FAIL: top-down render"; exit 1; }
python3 - "$tmp/a.png" "$tmp/b.png" "$tmp/w.png" "$(grep '^PROJECTED ' "$tmp/e1.log")" <<'PY' || exit 1
import struct,sys,zlib
def png(path):
    b=open(path,'rb').read(); assert b[:8]==b'\x89PNG\r\n\x1a\n', "not a PNG"
    i=8; w=h=0; idat=b''; ct=6
    while i<len(b):
        ln,typ=struct.unpack_from('>I4s',b,i); body=b[i+8:i+8+ln]; i+=12+ln
        if typ==b'IHDR': w,h,_,ct=struct.unpack('>IIBB',body[:10])
        elif typ==b'IDAT': idat+=body
        elif typ==b'IEND': break
    nch=3 if ct==2 else 4; stride=w*nch; raw=zlib.decompress(idat); p=0; prev=bytearray(stride); rows=[]
    for _ in range(h):
        f=raw[p]; p+=1; line=bytearray(raw[p:p+stride]); p+=stride
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
w,h,nch,rows=png(sys.argv[1])
assert (w,h)==(480,320), f"--size ignored: {w}x{h}"
bg=rows[h-1][:nch]
def fg(rows):
    xs=[];ys=[];n=0
    for y,r in enumerate(rows):
        for x in range(0,w*nch,nch):
            if max(abs(r[x+k]-bg[k]) for k in range(3))>8: n+=1; xs.append(x//nch); ys.append(y)
    return n,((min(xs),max(xs),min(ys),max(ys)) if xs else None)
n,bb=fg(rows); frac=n/(w*h)
assert 0.05<frac<0.85, f"foreground fraction {frac:.3f} implausible (blank or zero-filled image?)"
# Every pixel, not only the edge columns: a framed house leaves those all background.
uniq={r[x:x+nch] for r in rows for x in range(0,w*nch,nch)}
assert len(uniq)>3, f"image is a flat fill: {len(uniq)} colours"
x0,x1,y0,y1=bb; px=[float(v) for v in sys.argv[4].replace(",", " ").split()[1:5]]  # PROJECTED x0,y0,x1,y1
assert abs(x0-px[0])<=3 and abs(x1-px[2])<=3 and abs(y0-px[1])<=3 and abs(y1-px[3])<=3, \
    f"foreground bbox {(x0,x1,y0,y1)} != projected bounds {px} (3px)"
w2,h2,n2_,rows2=png(sys.argv[2])
assert rows!=rows2, "orbiting changed nothing - no depth/geometry"
n2,_=fg(rows2); assert 0.05<n2/(w2*h2)<0.85
w3,h3,n3_,rows3=png(sys.argv[3]); n3,_=fg(rows3)
assert n3>n, f"top-down must cover more pixels ({n3}) than oblique ({n}) - depth test missing?"
print(f"ok: {w}x{h}, {frac:.3f} foreground, bbox {(x0,x1,y0,y1)}")
PY
echo "PASS 06"
