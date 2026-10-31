# Demo: the phase-1 house at night with four pot lights in the kitchen, orbited smoothly (no shadow map rebuilt), then a
# Finalize Render; the plan shows the four symbols on E-LITE.
# needs: p10-arch-light-placement, p10-arch-light-symbol, p10-arch3d-scene-lights, p10-arch3d-fixture-meshes, p10-cmd-lightfixture, p10-cmd-illuminance, p10-r3d-light-pack, p10-r3d-shadow-atlas, p10-r3d-lit-pipeline, p10-r3d-lit-frame, p10-r3d-finalize, p10-cmd-render, p10-cli-render-hook, p10-io-light-proxies
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())

HOUSE = [cmd('wall', {"points": [[0, 0], [10, 0], [10, 8], [0, 8], [0, 0]], "thickness": 0.2, "height": 2.4, "exterior": True}),
         cmd('wall', {"points": [[0, 3], [10, 3]], "thickness": 0.1, "height": 2.4, "exterior": False}),
         cmd('wall', {"points": [[6, 3], [6, 8]], "thickness": 0.1, "height": 2.4, "exterior": False}),
         'roomdetect {}', 'arch.inspect {"rooms":true}']
tmp = tempfile.mkdtemp(); dxf = os.path.join(tmp, 'kitchen.dxf')
rc, rr, so, se = run(HOUSE, save=dxf)
rooms = reply_with(rr, 'rooms')['rooms']


def ring_of(r):
    pts = r.get('polygon') or r.get('ring') or r.get('points') or []
    return [(float(p[0]), float(p[1])) if isinstance(p, list) else (float(p['x']), float(p['y'])) for p in pts]


def inside(ring, x, y):
    c = False
    for (x1, y1), (x2, y2) in zip(ring, ring[1:] + ring[:1]):
        if (y1 > y) != (y2 > y) and x < x1 + (y - y1) * (x2 - x1) / (y2 - y1):
            c = not c
    return c


kitchen = [r for r in rooms if inside(ring_of(r), 8.0, 5.5)]
if len(kitchen) != 1:
    fail(f"one room holds (8, 5.5), the kitchen: {[(r.get('id') or r.get('label'), ring_of(r)[:4]) for r in rooms]}")
kid = kitchen[0].get('label') or kitchen[0].get('id')
xs = [p[0] for p in ring_of(kitchen[0])]; ys = [p[1] for p in ring_of(kitchen[0])]
x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
centres = sorted((round(x0 + (i + 0.5) * (x1 - x0) / 2, 6), round(y0 + (j + 0.5) * (y1 - y0) / 2, 6)) for i in range(2) for j in range(2))
mid = [round((x0 + x1) / 2, 6), round((y0 + y1) / 2, 6), 0.0]
under = [centres[0][0], centres[0][1], 0.0]
print(f"kitchen {kid}: {x0:.3f}..{x1:.3f} x {y0:.3f}..{y1:.3f}; lights at {centres}")

CAM = lambda az: {"kind": "orbit", "target": [8, 5.5, 1.2], "azimuth": az, "elevation": 30, "distance": 1.4, "fov": 80}
INSIDE = {"kind": "full", "eye": [x0 + 0.3, y0 + 0.3, 2.0], "target": [mid[0] + 0.5, mid[1] + 0.5, 0.0], "fov": 80}
final = os.path.join(tmp, 'night-kitchen.png')
orbit = [cmd('render', {"size": [200, 150], "camera": CAM(200 + k), "sun": False}) for k in range(31)]
rc, rr, so, se = run([cmd('lightfixture', {"room": kid, "rows": 2, "cols": 2}), 'arch.inspect {"lights":true}',
                      cmd('illuminance', {"point": under})] + orbit + [
                      cmd('render', {"out": final, "size": [480, 360], "camera": INSIDE, "sun": False, "final": True,
                                     "samples": [under, mid]})], file=dxf, save=os.path.join(tmp, 'kitchen-lit.dxf'))
ls = lights_of(rr)
at = sorted((round(num(l['at'][0], 'x'), 6), round(num(l['at'][1], 'y'), 6)) for l in ls)
if len(ls) != 4 or at != centres:
    fail(f"four pot lights on an even 2 x 2 grid in the kitchen: {at}, want {centres}")
lux = reply_with(rr, 'contributions')['lux']
near(lux, 165.0, 1.65, 'illuminance under a kitchen pot light (2.4 m ceiling)')
r = renders(rr)
if len(r) != 32:
    fail(f"31 orbit frames and a final render, got {len(r)} render replies")
if num(r[0].get('frameShadowMaps'), 'first frame maps') != 4:
    fail(f"the first orbit frame builds the four maps: {r[0].get('frameShadowMaps')}")
if any(num(x.get('frameShadowMaps'), 'maps') != 0 for x in r[1:31]):
    fail(f"orbiting builds no shadow map: {[x.get('frameShadowMaps') for x in r[1:31]]}")
f = r[31]
if f.get('final') is not True or num(f.get('finalRenders'), 'finalRenders') != 1:
    fail(f"one Finalize Render: {f.get('final')}, {f.get('finalRenders')}")
bright = rgb_sum(f['samples'][0], 'kitchen floor under a pot light')
dark = rgb_sum(f['samples'][1], 'kitchen floor between the cones')
print(f"final: floor under a light {bright:.0f}, between the cones {dark:.0f} (r+g+b)")
if bright < dark + 90:
    fail(f"in the night kitchen the floor under a pot light is far brighter than between the cones: {bright:.0f} vs {dark:.0f}")
w, h, nch, rows = png(final)
if (w, h) != (480, 360):
    fail(f"the final still is 480 x 360, got {w} x {h}")
artifact(final, 'demo-night-kitchen.png')
text = dxf_text(os.path.join(tmp, 'kitchen-lit.dxf'))
if len([e for e in entities(text) if e[1] == 'E-LITE']) != 8:
    fail("the saved plan shows the four pot-light symbols (two rings each) on E-LITE")
print("PASS 11")
