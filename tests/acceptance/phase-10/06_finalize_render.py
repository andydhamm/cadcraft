# Finalize Render: every light shadowed (past the interactive budget), the column's shadow on the floor, a PNG, counted.
# needs: p10-arch-light-placement, p10-arch3d-scene-lights, p10-arch3d-fixture-meshes, p10-cmd-lightfixture, p10-r3d-light-pack, p10-r3d-shadow-atlas, p10-r3d-lit-pipeline, p10-r3d-lit-frame, p10-r3d-finalize, p10-cmd-render, p10-cli-render-hook, p10-cli-render-final, p10-io-light-proxies
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())

CAM = {"kind": "full", "eye": [3.6, 3.6, 2.0], "target": [1.8, 1.8, 0.0], "fov": 70}
tmp = tempfile.mkdtemp()
shots = {}
for name, extra in (('open', []), ('column', [COLUMN])):
    out = os.path.join(tmp, f'final-{name}.png')
    rc, rr, so, se = run(room_cmds() + [POTS] + extra + [
        cmd('render', {"size": [160, 120], "camera": CAM, "sun": False, "samples": [[1.8, 1.8, 0]]}),
        cmd('render', {"out": out, "size": [320, 240], "camera": CAM, "sun": False, "final": True, "samples": [[1.8, 1.8, 0], [3, 1, 0]]}),
        'build3d {}'])
    r = renders(rr)
    if len(r) != 2:
        fail(f"{name}: two render replies, got {len(r)}: {json.dumps(rr)[-600:]}")
    f = r[1]
    if f.get('final') is not True or num(f.get('finalRenders'), 'finalRenders') != 1 or num(r[0].get('finalRenders'), 'f') != 0:
        fail(f"{name}: one Finalize Render counted (0 before, 1 after): {r[0].get('finalRenders')}, {f.get('finalRenders')}")
    b = reply_with(rr, 'fixtures')
    if num(b.get('finalRenders'), 'build3d finalRenders') != 1:
        fail(f"{name}: build3d reports finalRenders 1, got {b.get('finalRenders')}")
    w, h, nch, rows = png(out)
    if (w, h) != (320, 240):
        fail(f"{name}: the final PNG is 320 x 240, got {w} x {h}")
    shots[name] = (rgb_sum(f['samples'][0], f'{name} final (1.8,1.8,0)'), rgb_sum(f['samples'][1], f'{name} final (3,1,0)'))
    artifact(out, f'final-{name}.png')
print(f"final (1.8,1.8,0): open {shots['open'][0]:.0f}, behind the column {shots['column'][0]:.0f}; (3,1,0): {shots['open'][1]:.0f} / {shots['column'][1]:.0f}")
if shots['column'][0] > shots['open'][0] - 60:
    fail(f"in the Finalize Render the column's shadow darkens (1.8,1.8,0) by 60+ in r+g+b: {shots}")
if abs(shots['column'][1] - shots['open'][1]) > 30:
    fail(f"(3,1,0), under lf2, is lit the same with or without the column: {shots}")
print("ok: the column's shadow in the final render; (3,1,0) unchanged")

# Every light shadowed in Finalize, past the interactive budget of 8.
rc, rr, so, se = run(room_cmds(w=6.0, h=4.0) + [cmd('lightfixture', {"room": "r0", "rows": 3, "cols": 4}),
                     cmd('render', {"size": [120, 90], "camera": CAM, "sun": False}),
                     cmd('render', {"size": [120, 90], "camera": CAM, "sun": False, "final": True})])
r = renders(rr)
if num(r[0].get('shadowedLights'), 'interactive shadowed') != 8 or num(r[1].get('shadowedLights'), 'final shadowed') != 12:
    fail(f"interactive shadows 8 of 12, Finalize all 12: {r[0].get('shadowedLights')}, {r[1].get('shadowedLights')}")
print("ok: Finalize shadows all 12 lights")

# Headless: cadcraft-cli render --night --final.
dxf = os.path.join(tmp, 'room.dxf'); still = os.path.join(tmp, 'still.png')
run(room_cmds() + [POTS], save=dxf)
p = cli('render', [dxf, still, '--night', '--final', '--size', '200x150', '--adapter', 'fallback',
                   '--camera', 'full,eye=3.6,3.6,2.0,target=1.8,1.8,0,fov=70'])
if 'FINAL renders=1' not in p.stderr or 'LIGHTS 4' not in p.stderr or 'SHADOWMAPS built=' not in p.stderr:
    fail(f"render --final prints FINAL renders=1, LIGHTS 4 … and SHADOWMAPS built=…: {p.stderr[-600:]}")
w, h, nch, rows = png(still)
if (w, h) != (200, 150):
    fail(f"render --final writes the requested size: {w} x {h}")
p2 = cli('render', [dxf, still, '--night', '--sun', 'date=2026-06-21,time=12:00,lat=51'], expect_ok=False)
if p2.returncode == 0:
    fail("--night with --sun is refused")
print("PASS 06")
