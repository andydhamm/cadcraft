#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'window {"wall":"w2","offset":4.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}' \
  --cmd 'roomdetect {}' --cmd 'roofauto {"pitch":[6,12]}' --save "$tmp/p.dxf" >/dev/null 2>&1 \
  || { echo "FAIL: run failed"; exit 1; }
CAM='orbit,azimuth=35,elevation=30,distance=25'
for st in shaded shaded-edges hidden-line glass-house; do
  cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/$st.png" --size 480x320 \
    --camera "$CAM" --style "$st" --adapter fallback 2>"$tmp/$st.log" \
    || { echo "FAIL: render --style $st"; head -3 "$tmp/$st.log"; exit 1; }
  grep -q '^PROJECTED ' "$tmp/$st.log" || { echo "FAIL: --style $st no PROJECTED"; exit 1; }
done
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/x.png" --size 480x320 --style bogus 2>/dev/null
[ $? -ne 0 ] || { echo "FAIL: --style bogus accepted"; exit 1; }
python3 - "$tmp" <<'PY' || exit 1
import struct,sys,zlib
def png(p):
    b=open(p,'rb').read(); assert b[:8]==b'\x89PNG\r\n\x1a\n'
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
tmp=sys.argv[1]
def stats(k):
    w,h,nch,rows=png(f"{tmp}/{k}.png"); assert (w,h)==(480,320), k
    bg=rows[h-1][:nch]; xs=[];ys=[];n=0;uniq=set();lum=0
    for y,r in enumerate(rows):
        for x in range(0,w*nch,nch):
            c=r[x:x+nch]; uniq.add(c)
            if max(abs(c[j]-bg[j]) for j in range(3))>8:
                n+=1; xs.append(x//nch); ys.append(y); lum+=sum(c[:3])
    return dict(frac=n/(w*h),uniq=len(uniq),bbox=(min(xs),max(xs),min(ys),max(ys)),
                lum=lum/(3*n) if n else 0,rows=rows)
s={k:stats(k) for k in ["shaded","shaded-edges","hidden-line","glass-house"]}
for k,v in s.items():
    lo = 0.002 if k == "hidden-line" else 0.05     # hidden line: visible edges on the background, faces in the clear colour
    assert lo<v["frac"]<0.85, f"{k}: fraction {v['frac']:.3f} (blank/zero-filled?)"
    assert v["uniq"]>(1 if k == "hidden-line" else 3), f"{k}: flat fill"   # hidden line: background + edge colour
    px=[float(x) for x in open(f"{tmp}/{k}.log").read().split('PROJECTED')[1].split('\n')[0].replace(',',' ').split()[:4]]
    x0,x1,y0,y1=v["bbox"]
    assert abs(x0-px[0])<=3 and abs(x1-px[2])<=3 and abs(y0-px[1])<=3 and abs(y1-px[3])<=3, \
        f"{k}: bbox {(x0,x1,y0,y1)} != PROJECTED {px}"
b=s["shaded"]
for k in ["shaded-edges","hidden-line","glass-house"]:
    assert s[k]["rows"]!=b["rows"], f"--style {k} identical to shaded: flag ignored"
e=s["shaded-edges"]["rows"]; n_=len(e[0])//480   # edge lines replace face colours, so count changed pixels, not colours
changed=sum(1 for y in range(len(e)) for x in range(0,len(e[y]),n_) if max(abs(e[y][x+k]-b["rows"][y][x+k]) for k in range(3))>24)
assert changed>300, f"shaded-edges adds no edge pixels ({changed} pixels differ from shaded)"
assert s["shaded-edges"]["lum"]<b["lum"], "feature edges must darken"
assert s["hidden-line"]["frac"]<0.7*b["frac"], f"hidden-line {s['hidden-line']['frac']:.3f} vs shaded {b['frac']:.3f}"
assert s["glass-house"]["uniq"]>b["uniq"], "glass-house produces no blended colours"
print("ok: styles differ", {k:(round(v['frac'],3),v['uniq']) for k,v in s.items()})
PY
echo "PASS 02"
