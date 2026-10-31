# The demo: the phase-1 house on a sloping site with a pad, a driveway, a retaining wall and trees, rendered headless.
# needs: p9-cmd-terrain, p9-cmd-terrain-modify, p9-arch-terrain-pad, p9-cmd-wall-retaining, p9-arch-retaining, p9-cmd-plant, p9-arch3d-plants, p9-arch3d-terrain-solid, p9-render-site-plan, p9-cli-render-sun-from-site, p9-io-site-proxies
import os, re, shutil
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

tmp = tempfile.mkdtemp(); dxf = os.path.join(tmp, 'demo.dxf'); glbp = os.path.join(tmp, 'demo.glb')
demo = [cmd('site', {"latitude": 51.05, "longitude": -114.07, "trueNorth": 15}),
        cmd('wall', {"points": [[15, 11], [25, 11], [25, 19], [15, 19], [15, 11]], "thickness": 0.2, "height": 2.4, "exterior": True}),
        cmd('wall', {"points": [[15, 14], [25, 14]], "thickness": 0.1, "height": 2.4, "exterior": False}),
        cmd('wall', {"points": [[21, 14], [21, 19]], "thickness": 0.1, "height": 2.4, "exterior": False}),
        cmd('door', {"wall": "w0", "offset": 3.0, "width": 0.9, "height": 2.1, "style": "hinged"}),
        'roomdetect {}', cmd('roofauto', {"pitch": [6, 12], "overhang": 0.5}),
        terrain_cmd(),
        cmd('terrain.modify', {"path": {"kind": "driveway", "polyline": [[18, 0], [18, 10.9]], "width": 3}}),
        cmd('terrain.modify', {"flat": {"polygon": [[32, 0], [40, 0], [40, 30], [32, 30]], "elevation": 2.4}}),
        cmd('wall', {"points": [[32, 0], [32, 30]], "thickness": 0.25, "retaining": True, "exterior": False}),
        cmd('plant', {"species": "Sugar Maple", "at": [8, 24], "age": 20}),
        cmd('plant', {"species": "Colorado Blue Spruce", "at": [28, 25], "age": 12}),
        cmd('plant', {"species": "Common Lilac", "at": [13, 9], "age": 4}),
        cmd('build3d', {"probe": [[20, 12.5, -0.15], [18, 5, 0.95], [8, 24, 9], [32, 15, 2.5]]})]
rc, rr, so, se = run(demo, save=dxf)
b = probes(rr)
for k, n in (('terrain', 1), ('paths', 1), ('plants', 3), ('roofs', 1)):
    if num(b.get(k), f'build3d {k}') < n:
        fail(f"the demo needs {n} {k} in 3D, got {b.get(k)}")
want = ['floor', 'path', 'plant', 'wall']
got = [part(h) for h in b['probe']]
if got != want:
    fail(f"probes (floor slab on the pad, driveway, maple crown, retaining wall): expected {want}, got {got}")
cli('convert', [dxf, glbp, '--no-detect'])
j, binc = glb(glbp)
names = {n.get('name') for n in j.get('nodes', [])}
for n in ('terrain', 'driveway-tm0', 'plant-pl0', 'plant-pl1', 'plant-pl2'):
    if n not in names:
        fail(f"GLB node {n} missing; nodes: {sorted(x for x in names if x)[:40]}")
if not any(x and x.startswith('roof-') for x in names) or not any(x and x.startswith('wall-') for x in names):
    fail("the house (walls, roof) must be in the same GLB as the site")
print("ok: house on its pad, driveway, retaining wall, three plants; GLB has the site and the house")

out = os.path.join(tmp, 'demo.png')
p = cli('render', [dxf, out, '--size', '960x640', '--style', 'shaded', '--sun', 'date=2026-06-21,time=15:00',
                   '--camera', 'orbit,azimuth=215,elevation=28,distance=70,target=20,15,1', '--adapter', 'fallback'])
m = re.search(r'PROJECTED\s+([-\d.]+),([-\d.]+),([-\d.]+),([-\d.]+)', p.stdout + p.stderr)
w, h, nch, rows = png(out)
bg = rows[2][0:nch]
fg = sum(1 for r in rows for x in range(0, w * nch, nch) if any(abs(r[x + c] - bg[c]) > 8 for c in range(min(3, nch))))
frac = fg / (w * h)
if not (0.15 < frac < 0.95):
    fail(f"demo render foreground {frac:.1%} outside (15%, 95%): the site and the house should fill much of the frame")
if not m:
    fail("render must print PROJECTED x0,y0,x1,y1")
art = os.environ.get('CHECK_ARTIFACTS_DIR')
if art:
    os.makedirs(art, exist_ok=True); shutil.copy(out, os.path.join(art, 'demo-house-on-site.png'))
print(f"ok: demo.png {w}x{h}, foreground {frac:.1%}")
print("PASS 12")
