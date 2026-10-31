# The site in the saved plan (proxies for other programs) and a lossless round trip.
# needs: p9-io-site-proxies, p9-render-site-plan, p9-cmd-terrain, p9-cmd-terrain-modify, p9-cmd-plant, p9-cmd-site, p9-cmd-wall-retaining, p9-cmd-site-inspect
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

tmp = tempfile.mkdtemp(); a = os.path.join(tmp, 'a.dxf'); b = os.path.join(tmp, 'b.dxf')
INSPECT = 'arch.inspect {"site":true,"terrain":true,"plants":true,"walls":true}'
build = [cmd('site', {"latitude": 51.05, "trueNorth": 30, "reference": {"point": [0, 0], "easting": 500000, "northing": 5650000, "crs": "EPSG:32611"}}),
         terrain_cmd(),
         cmd('terrain.modify', {"flat": {"polygon": [[30, 0], [40, 0], [40, 30], [30, 30]], "elevation": 2.5}}),
         cmd('terrain.modify', {"path": {"kind": "sidewalk", "polyline": [[2, 2], [12, 2]], "width": 1.2}}),
         cmd('wall', {"points": [[30, 0], [30, 30]], "thickness": 0.2, "retaining": True, "exterior": False}),
         cmd('plant', {"species": "Sugar Maple", "at": [8, 22], "age": 10}), INSPECT]
rc, rr, so, se = run(build, save=a)
before = reply_with(rr, 'terrain', -1)
rc, rr, so, se = run([INSPECT], file=a, save=b)
after = reply_with(rr, 'terrain', -1)
for k in ('site', 'terrain', 'plants', 'walls'):
    x, y = json.dumps(before.get(k), sort_keys=True), json.dumps(after.get(k), sort_keys=True)
    if x != y:
        fail(f"arch.inspect {k} changed across save and open:\n before {x[:400]}\n after  {y[:400]}")
print("ok: site, terrain, modifiers, retaining wall and plants survive save + open")

def layer_counts(path):
    c = {}
    for kind, layer, pairs in entities(dxf_text(path)):
        c[(kind, layer)] = c.get((kind, layer), 0) + 1
    return c

ca, cb = layer_counts(a), layer_counts(b)
if ca != cb:
    fail(f"opening and saving again must not duplicate site proxies: {sorted(set(ca.items()) ^ set(cb.items()))[:12]}")
need = {('LWPOLYLINE', 'C-TOPO-BNDY'): 1, ('LWPOLYLINE', 'C-TOPO-MAJR'): 1, ('LWPOLYLINE', 'C-ROAD'): 1,
        ('LWPOLYLINE', 'C-TOPO-GRAD'): 1, ('CIRCLE', 'L-PLNT'): 1, ('LWPOLYLINE', 'A-ANNO-NRTH'): 1}
for k, n in need.items():
    if ca.get(k, 0) < n:
        fail(f"the saved plan needs a {k[0]} on {k[1]}; layers present: {sorted(ca)}")
texts = [values(p, '1') for kind, layer, p in entities(dxf_text(a)) if kind == 'TEXT']
flat = [t for v in texts for t in v]
if '1.00' not in flat or 'N' not in flat:
    fail(f"the plan labels the major contour '1.00' (C-TOPO-TEXT) and the north arrow 'N': texts {flat[:20]}")
minor = ca.get(('LWPOLYLINE', 'C-TOPO-MINR'), 0)
if minor < 4:
    fail(f"minor contours west of the flat region (x = 5..25 at 0.25 m, the 1.00 major drawn on C-TOPO-MAJR): got {minor}")
print(f"ok: proxies on C-TOPO-BNDY/MINR({minor})/MAJR/TEXT/GRAD, C-ROAD, L-PLNT, A-ANNO-NRTH; no duplicates after open + save")
print("PASS 09")
