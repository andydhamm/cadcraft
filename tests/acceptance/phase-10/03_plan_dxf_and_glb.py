# Plan symbols on E-LITE, fixtures round-trip in the DXF record, and the GLB carries KHR_lights_punctual.
# needs: p10-arch-light-symbol, p10-io-light-proxies, p10-io-glb-lights, p10-cmd-lightfixture, p10-arch3d-scene-lights, p10-arch3d-fixture-meshes
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())

tmp = tempfile.mkdtemp(); dxf = os.path.join(tmp, 'lights.dxf'); dxf2 = os.path.join(tmp, 'again.dxf'); out = os.path.join(tmp, 'lights.glb')
rc, rr, so, se = run(room_cmds() + [POTS], save=dxf)
text = dxf_text(dxf)
lite = [e for e in entities(text) if e[1] == 'E-LITE']
if not lite or any(e[0] != 'LWPOLYLINE' for e in lite):
    fail(f"the fixtures' plan symbols are LWPOLYLINEs on E-LITE: {[(e[0], e[1]) for e in lite][:10]}")
if len(lite) != 8:
    fail(f"four pot lights draw two rings each (trim and lens) on E-LITE: {len(lite)} polylines")
# Each pot light's outer ring is a circle of radius 0.075 around its centre.
radii = []
for e in lite:
    xs = [float(v) for v in values(e[2], '10')]; ys = [float(v) for v in values(e[2], '20')]
    cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    radii.append(max(math.hypot(x - cx, y - cy) for x, y in zip(xs, ys)))
    if min(abs(cx - a) + abs(cy - b) for a, b in ((1, 1), (1, 3), (3, 1), (3, 3))) > 1e-3:
        fail(f"a symbol centred at ({cx:.4f}, {cy:.4f}) is not on a fixture")
if any(min(abs(r - 0.075), abs(r - 0.0375)) > 1e-4 for r in radii) \
        or sum(abs(r - 0.075) <= 1e-4 for r in radii) != 4:
    fail(f"each pot light draws a circle of radius 0.075 and one of 0.0375: {[round(r, 5) for r in radii]}")
print("ok: 8 symbol rings on E-LITE, radius 0.075 and 0.0375, centred on the fixtures")

rc, rr, so, se = run(['arch.inspect {"lights":true}'], file=dxf, save=dxf2)
ls = lights_of(rr)
if [l.get('id') for l in ls] != ['lf0', 'lf1', 'lf2', 'lf3'] or any(abs(num(l.get('z'), 'z') - 2.4) > 1e-9 for l in ls):
    fail(f"reopening the DXF keeps the four fixtures under the 2.4 m ceiling: {ls}")
text2 = dxf_text(dxf2)
again = [e for e in entities(text2) if e[1] == 'E-LITE']
if len(again) != 8:
    fail(f"open + save never duplicates the light proxies: {len(again)} polylines on E-LITE after a second save (want 8)")
print("ok: the fixtures round-trip; open + save keeps 8 proxies")

cli('convert', [dxf, out, '--no-detect'])
j, binc = glb(out)
if 'KHR_lights_punctual' not in j.get('extensionsUsed', []):
    fail(f"the GLB declares KHR_lights_punctual: extensionsUsed {j.get('extensionsUsed')}")
L = j.get('extensions', {}).get('KHR_lights_punctual', {}).get('lights', [])
if len(L) != 4 or any(x.get('type') != 'spot' for x in L):
    fail(f"four spot lights in the GLB: {L}")
for x in L:
    near(x.get('intensity'), 950.36, 0.05, 'GLB light intensity (candela)')
    near(x.get('spot', {}).get('outerConeAngle'), math.pi / 6, 1e-6, 'GLB outer cone angle (half of 60°)')
names = [n.get('name') for n in j.get('nodes', [])]
src = [n for n in j.get('nodes', []) if n.get('name') == 'light-lf0-source']
if not src or 'light-lf0' not in names:
    fail(f"the GLB names the pot light's housing 'light-lf0' and its light node 'light-lf0-source': {names[:40]}")
t = src[0].get('translation', [])
if len(t) != 3 or abs(t[0] - 1) > 1e-6 or abs(t[1] - 1) > 1e-6 or abs(t[2] - 2.4) > 1e-6:
    fail(f"light-lf0-source sits at (1, 1, 2.4) under the Z-up root: {t}")
root = j['nodes'][0]
idx = names.index('light-lf0-source')
if idx not in root.get('children', []):
    fail("the light nodes are children of the root (so its Z-up to Y-up rotation applies)")
print("PASS 03")
