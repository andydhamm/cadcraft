# Contours: minor every 0.25 m, major every 1.0 m, chained into polylines, the major labelled.
# needs: p9-cmd-terrain, p9-cmd-terrain-modify, p9-cmd-terrain-query, p9-arch-terrain-contours
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

rc, rr, so, se = run([terrain_cmd(), 'terrain.contours {}'])
c = reply_with(rr, 'minor')
mi, ma = c.get('minor') or {}, c.get('major') or {}
near(mi.get('interval'), 0.25, 1e-12, 'minor interval'); near(ma.get('interval'), 1.0, 1e-12, 'major interval')
lines = mi.get('lines') or []
if len(lines) != 7:
    fail(f"the sloped site has seven minor contours (x = 5, 10, ..., 35), got {len(lines)}: {[l.get('elevation') for l in lines]}")
for want_e, l in zip([0.25 * k for k in range(1, 8)], sorted(lines, key=lambda l: l['elevation'])):
    near(l['elevation'], want_e, 1e-9, 'minor contour elevation')
    x = want_e / 0.05
    ps = l.get('points') or []
    if len(ps) < 2 or l.get('closed'):
        fail(f"contour {want_e} must be one open polyline: {l}")
    for p in ps:
        if abs(num(p[0], 'x') - x) > 1e-6:
            fail(f"contour {want_e} must lie on x = {x}: point {p}")
    ys = sorted(num(p[1], 'y') for p in ps)
    if ys[0] > 1e-6 or ys[-1] < 30 - 1e-6:
        fail(f"contour {want_e} must run the full depth y 0..30 as ONE chained polyline, spans {ys[0]}..{ys[-1]}")
majors = ma.get('lines') or []
if len(majors) != 1:
    fail(f"one major contour (1.00 at x = 20), got {len(majors)}")
m = majors[0]
near(m['elevation'], 1.0, 1e-9, 'major elevation')
if m.get('label') != '1.00':
    fail(f"the major contour's label is '1.00', got {m.get('label')!r}")
la = m.get('labelAt')
if not isinstance(la, list) or abs(num(la[0], 'labelAt x') - 20) > 1e-6 or not (0 < num(la[1], 'labelAt y') < 30):
    fail(f"the label sits on the line x = 20 inside the site: {la}")
print("ok: seven minor contours x = 5..35, one major at x = 20 labelled 1.00")

# Intervals come from the terrain; a hill makes closed contours.
rc, rr, so, se = run([terrain_cmd(contours=[0.5, 2.0]), cmd('terrain.modify', {"hill": {"center": [20, 15], "height": 3, "radius": 6}}),
                      'terrain.contours {}'])
c = reply_with(rr, 'minor')
if num(c['minor']['interval'], 'minor') != 0.5 or num(c['major']['interval'], 'major') != 2.0:
    fail(f"contour intervals must follow terrain {{contours:[0.5,2.0]}}: {c['minor']['interval']}, {c['major']['interval']}")
if not any(l.get('closed') for l in c['minor']['lines']):
    fail("a hill in the middle of the site must give closed contour loops")
print("ok: intervals from the terrain; a hill gives closed loops")
print("PASS 02")
