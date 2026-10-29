#!/usr/bin/env bash
set -uo pipefail
before=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'camera {"save":"keep","kind":"orbit","distance":20}' \
  --cmd 'arch.inspect {"walls":true}' --cmd 'camera {"list":true}' 2>/dev/null | tail -n 2)
bad=(
 'pick3d {}'
 'pick3d {"camera":{"kind":"full","eye":[0,0,0],"target":[0,0,0],"fov":45},"size":[480,320],"pixel":[240,160]}'
 'pick3d {"camera":{"kind":"orbit","azimuth":1e308,"distance":10},"size":[480,320],"pixel":[240,160]}'
 'pick3d {"camera":{"kind":"orbit","distance":10},"size":[0,0],"pixel":[240,160]}'
 'pick3d {"camera":{"kind":"orbit","distance":10},"size":[480,320],"pixel":[1e9,0]}'
 'pick3d {"camera":{"kind":"bogus"},"size":[480,320],"pixel":[0,0]}'
 'camera {"save":"","kind":"full","eye":[0,0,1.6],"target":[0,1,1.6],"fov":45}'
 'camera {"save":"x","kind":"full","eye":[0,0,0],"target":[0,0,0],"fov":45}'
 'camera {"save":"x","kind":"full","eye":[0,0,0],"target":[0,1,0],"fov":180}'
 'camera {"save":"x","kind":"full","eye":[1e308,0,0],"target":[0,1,0],"fov":45}'
 'camera {"use":"nope"}' 'camera {"delete":"nope"}' 'camera {}'
 'sun {"lat":"north","date":"2026-06-21","time":"solar-noon"}'
 'sun {"lat":51.05,"date":"2026-13-45","time":"solar-noon"}'
 'sun {"lat":51.05,"date":"2026-06-21","time":"25:99"}'
 'sun {"lat":1e308,"date":"2026-06-21","time":"solar-noon"}'
 'walkthrough {"eye":[0,0,0],"yaw":"turn","forward":1,"strafe":0}'
 'walkthrough {"eye":[0,0,0],"yaw":0,"forward":1e308,"strafe":0,"turn":0}'
)
for c in "${bad[@]}"; do
  r=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$c" 2>&1)
  echo "$r" | grep -qi 'panicked\|index out of\|attempt to\|unwrap' && { echo "FAIL: panic on '$c'"; echo "$r" | head -5; exit 1; }
done
for st in bogus '' 'Hidden-Line'; do
  cargo run -q -p cadcraft-cli -- render /dev/null /tmp/p6x.png --style "$st" 2>/dev/null
  [ $? -ne 0 ] || { echo "FAIL: --style '$st' accepted"; exit 1; }
done
for a in nonsense '' 'fallback;rm'; do
  cargo run -q -p cadcraft-cli -- render /dev/null /tmp/p6x.png --adapter "$a" 2>/dev/null
  [ $? -ne 0 ] || { echo "FAIL: --adapter '$a' accepted"; exit 1; }
done
for s in 'orbit,elevation=95' 'orbit,fov=0' 'full,eye=NaN,0,0,target=0,1,0,fov=45' 'full,eye=0,0,0,target=0,0,0,fov=45' 'bogus'; do
  cargo run -q -p cadcraft-cli -- render /dev/null /tmp/p6x.png --camera "$s" 2>/dev/null
  [ $? -ne 0 ] || { echo "FAIL: --camera '$s' accepted"; exit 1; }
done
after=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'camera {"save":"keep","kind":"orbit","distance":20}' \
  --cmd 'arch.inspect {"walls":true}' --cmd 'camera {"list":true}' 2>/dev/null | tail -n 2)
[ "$before" = "$after" ] || { echo "FAIL: hostile input changed the model"; exit 1; }
echo "PASS 08"
