# Site location: true north turns the north arrow and the sun; a pole's solar-noon shadow points along the arrow.
# needs: p9-cmd-site, p9-arch-site, p9-cmd-site-inspect, p9-cli-render-sun-from-site
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

rc, rr, so, se = run([cmd('site', {"latitude": 51.05, "longitude": -114.07, "elevation": 1045, "trueNorth": 30}),
                      cmd('sun', {"date": "2026-06-21", "time": "solar-noon"}), 'arch.inspect {"site":true}'])
s = reply_with(rr, 'site', -1)['site']
for k, v in (('latitude', 51.05), ('longitude', -114.07), ('elevation', 1045), ('trueNorth', 30)):
    near(s.get(k), v, 1e-9, f"site {k}")
arrow = (s.get('northArrow') or {}).get('direction')
if not isinstance(arrow, list):
    fail(f"site must report northArrow.direction: {s}")
near(arrow[0], -0.5, 1e-9, 'north arrow x'); near(arrow[1], math.sqrt(3) / 2, 1e-9, 'north arrow y')
sun = reply_with(rr, 'dir')
near(sun.get('trueNorth'), 30, 1e-9, 'sun trueNorth')
d = sun['dir']; L = math.hypot(d[0], d[1])
shadow = [-d[0] / L, -d[1] / L]
if abs(shadow[0] - arrow[0]) > 1e-3 or abs(shadow[1] - arrow[1]) > 1e-3:
    fail(f"at solar noon a pole's shadow points true north, along the arrow {arrow}: shadow {shadow}")
print(f"ok: north arrow {arrow}, solar-noon shadow {[round(x, 4) for x in shadow]}")

rc, rr, so, se = run([cmd('sun', {"lat": 51.05, "date": "2026-06-21", "time": "solar-noon"})])
d = reply_with(rr, 'dir')['dir']
if abs(d[0]) > 1e-3 or d[1] > -0.1:
    fail(f"without a site, sun dir is unchanged (the sun due south, dir y < 0): {d}")
rc, rr, so, se = run([cmd('sun', {"date": "2026-06-21", "time": "solar-noon"})], expect_ok=False)
if rc == 0:
    fail("sun without lat and without a site must be refused")
for bad in ({"latitude": 91}, {"longitude": -181}, {"trueNorth": "x"}, {"elevation": None, "bogus": 1}):
    rc, rr, so, se = run([cmd('site', {"latitude": 10}), cmd('site', bad), 'arch.inspect {"site":true}'], expect_ok=False)
    s = reply_with(rr, 'site', -1)['site']
    if not was_refused(rc, se) or not s or num(s.get('latitude'), 'latitude') != 10:
        fail(f"site {bad} must be refused and change nothing: {s}")
print("ok: no-site sun unchanged; bad site values refused")

tmp = tempfile.mkdtemp(); dxf = os.path.join(tmp, 'pole.dxf'); png_path = os.path.join(tmp, 'pole.png')
run([cmd('site', {"latitude": 51.05, "trueNorth": 30}), cmd('wall', {"points": [[0, 0], [0.3, 0]], "thickness": 0.3, "height": 3})], save=dxf)
p = cli('render', [dxf, png_path, '--size', '320x240', '--sun', 'date=2026-06-21,time=solar-noon', '--ground',
                   '--camera', 'orbit,azimuth=0,elevation=89,distance=20,target=0.15,0,0', '--adapter', 'fallback'], expect_ok=False)
if p.returncode != 0:
    fail(f"render --sun without lat= must take the site's latitude: rc {p.returncode}, {(p.stderr or p.stdout)[-300:]}")
print("ok: render --sun takes the latitude from the site")
print("PASS 08")
