#!/usr/bin/env bash
# Headless wgpu may have no adapter here: that is a SKIP (exit 0), never a failure.
set -uo pipefail
tmp=$(mktemp -d); trap '[ -n "${CHECK_ARTIFACTS_DIR:-}" ] && cp "$tmp"/*.png "$CHECK_ARTIFACTS_DIR"/ 2>/dev/null; rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --save "$tmp/p.dxf" >/dev/null 2>&1 || { echo "FAIL: run failed"; exit 1; }
log=$(cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/a.png" --camera 'orbit,azimuth=35,elevation=30,distance=25' 2>&1)
if [ ! -s "$tmp/a.png" ]; then
  case "$log" in *adapter*|*gpu*|*NoAdapter*|*not\ available*) echo "SKIP 06: no headless GPU adapter"; exit 0;; esac
  echo "FAIL: render produced no png"; echo "$log" | head -20; exit 1
fi
python3 - "$tmp/a.png" <<'PY' || exit 1
import struct,sys,zlib
b=open(sys.argv[1],'rb').read()
assert b[:8]==b'\x89PNG\r\n\x1a\n', "not a PNG"
i=8; w=h=0; idat=b''; ct=6
while i<len(b):
    ln,typ=struct.unpack_from('>I4s',b,i); body=b[i+8:i+8+ln]; i+=12+ln
    if typ==b'IHDR': w,h,_,ct=struct.unpack('>IIBB',body[:10])
    elif typ==b'IDAT': idat+=body
    elif typ==b'IEND': break
assert w>=320 and h>=240, f"image too small: {w}x{h}"
nch=3 if ct==2 else 4; stride=w*nch; raw=zlib.decompress(idat)
prev=bytearray(stride); p=0; lit=0
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
    lit+=sum(1 for k in range(0,stride,nch) if line[k]<240)
    prev=line
assert lit>500, f"image is blank ({lit} dark pixels) - nothing was rendered"
print(f"ok: {w}x{h}, {lit} shaded pixels")
PY
echo "PASS 06"
