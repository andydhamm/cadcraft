# Flat regions, hills, valleys and draped paths; cut and fill between the original and the modified surface.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

SQ = [[15, 10], [25, 10], [25, 20], [15, 20]]
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"flat": {"polygon": SQ, "elevation": 1.0, "margin": 0}}),
                      'terrain.volumes {}', cmd('terrain.query', {"points": [[16, 15], [24, 15], [14, 15], [26, 15]]}),
                      cmd('terrain.query', {"points": [[16, 15]], "surface": "original"})])
if reply_with(rr, 'modifier').get('modifier') != 'tm0':
    fail(f"the first modifier is tm0: {reply_with(rr, 'modifier')}")
v = reply_with(rr, 'cut')
near(v['cut'], 6.25, 1e-3, 'cut (m³)'); near(v.get('fill'), 6.25, 1e-3, 'fill (m³)')
e = elevations(rr, 0)
for got, want, what in zip(e, [1.0, 1.0, 0.7, 1.3], ['(16,15) inside', '(24,15) inside', '(14,15) outside', '(26,15) outside']):
    near(got, want, 1e-6, f"modified elevation at {what}")
near(elevations(rr, 1)[0], 0.8, 1e-6, 'original elevation at (16,15)')
print("ok: flat 1.0 over (15,10)-(25,20): cut 6.25 m³, fill 6.25 m³")

rc, rr, so, se = run([terrain_cmd(), 'terrain.volumes {}'])
v = reply_with(rr, 'cut')
near(v['cut'], 0.0, 1e-9, 'cut without modifiers'); near(v['fill'], 0.0, 1e-9, 'fill without modifiers')

# Margin: the surface blends back linearly over the margin.
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"flat": {"polygon": SQ, "elevation": 1.0, "margin": 2}}),
                      cmd('terrain.query', {"points": [[14, 15], [13, 15], [12.5, 15]]})])
e = elevations(rr)
near(e[0], 1.0 + (0.70 - 1.0) * 0.5, 1e-6, 'margin 2: 1 m out'); near(e[1], 0.65, 1e-6, 'margin 2: at the margin edge')
near(e[2], 0.625, 1e-6, 'margin 2: beyond the margin')
print("ok: margin blends back to the ground")

# Hill and valley with the smooth falloff (1 - t²)².
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"hill": {"center": [10, 20], "height": 2, "radius": 5}}),
                      cmd('terrain.modify', {"valley": {"center": [30, 10], "depth": 1, "radius": 4}}),
                      cmd('terrain.query', {"points": [[10, 20], [13, 20], [16, 20], [30, 10], [32, 10]]}),
                      'arch.inspect {"terrain":true}'])
e = elevations(rr)
for got, want, what in zip(e, [2.5, 0.65 + 2 * 0.4096, 0.8, 0.5, 1.6 - 0.5625], ['hill top', 'hill t=0.6', 'beyond the hill', 'valley bottom', 'valley t=0.5']):
    near(got, want, 1e-6, what)
mods = (reply_with(rr, 'terrain', -1).get('terrain') or {}).get('modifiers') or []
if [m.get('id') for m in mods] != ['tm0', 'tm1'] or [m.get('type') for m in mods] != ['hill', 'hill'] or num(mods[1].get('height'), 'valley height') != -1:
    fail(f"arch.inspect terrain modifiers: hill tm0, valley tm1 stored as a hill of height -1: {mods}")
print("ok: hill +2 r5 and valley -1 r4 follow (1 - t²)²")

# A driveway from a polyline and a width, draped on the ground 0.1 thick.
tmp = tempfile.mkdtemp(); dxf = os.path.join(tmp, 'drive.dxf'); out = os.path.join(tmp, 'drive.glb')
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"path": {"kind": "driveway", "polyline": [[20, 0], [20, 10]], "width": 3}}),
                      cmd('build3d', {"probe": [[20, 5, 1.05], [20, 5, 1.15], [20, 5, 0.95], [23, 5, 1.2]]}), 'arch.inspect {"terrain":true}'],
                     save=dxf)
b = probes(rr)
if num(b.get('paths'), 'build3d paths') != 1:
    fail(f"build3d must count one path, got {b.get('paths')}")
if [part(h) for h in b['probe']] != ['path', None, 'terrain', None]:
    fail(f"driveway draped 0.1 thick on the ground at 1.0: probes {b['probe']}")
m = (reply_with(rr, 'terrain', -1)['terrain'] or {}).get('modifiers') or [{}]
poly = sorted(tuple(round(num(c, 'xy'), 6) for c in q) for q in m[0].get('polygon') or [])
if m[0].get('kind') != 'driveway' or poly != sorted([(18.5, 0.0), (21.5, 0.0), (21.5, 10.0), (18.5, 10.0)]):
    fail(f"the driveway polygon is the 3 m band around the polyline: {m[0]}")
cli('convert', [dxf, out, '--no-detect'])
j, binc = glb(out)
node_box(j, 'driveway-tm0')
print("ok: driveway 3 m wide, draped 0.1 m thick, GLB node driveway-tm0")

rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"hill": {"center": [10, 20], "height": 2, "radius": 5}}),
                      cmd('terrain.modify', {"delete": "tm0"}), cmd('terrain.query', {"points": [[10, 20]]})])
near(elevations(rr)[0], 0.5, 1e-6, 'after deleting the hill')
print("ok: delete tm0")
print("PASS 04")
