# Light fixtures: the catalogue, one fixture at a point, an even grid in a room, heights by mount, housings in 3D.
# needs: p10-arch-light-placement, p10-cmd-lightfixture, p10-arch3d-scene-lights, p10-arch3d-fixture-meshes
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())

rc, rr, so, se = run(['lightfixture.types {}'])
types = reply_with(rr, 'types')['types']
names = [t.get('name') for t in types]
for n in ("Recessed pot light", "Pendant", "Wall sconce", "Surface ceiling fixture"):
    if n not in names:
        fail(f"the catalogue needs '{n}': {names}")
pot = types[names.index("Recessed pot light")]
if pot.get('kind') != 'spot' or num(pot.get('cone'), 'cone') != 60 or num(pot.get('lumens'), 'lumens') != 800 or num(pot.get('kelvin'), 'K') != 3000:
    fail(f"the recessed pot light is a spot with a 60° cone, 800 lm, 3000 K: {pot}")
print(f"ok: {len(types)} fixture types, the pot light is a 60° 800 lm 3000 K spot")

rc, rr, so, se = run(room_cmds() + [POTS,
                                    cmd('lightfixture', {"type": "Pendant", "at": [2, 2]}),
                                    cmd('lightfixture', {"type": "Wall sconce", "at": [0.0, 2.0], "rotation": 0}),
                                    cmd('lightfixture', {"type": "Surface ceiling fixture", "at": [2, 3.5]}),
                                    'arch.inspect {"lights":true}',
                                    cmd('build3d', {"probe": [[1.065, 1.0, 2.395], [1.0, 1.0, 1.2], [2.0, 2.0, 1.85]]})])
grid = placed(rr, 0)
if grid != ['lf0', 'lf1', 'lf2', 'lf3']:
    fail(f"a 2 x 2 grid in the room places lf0..lf3: {grid}")
ls = lights_of(rr)
if len(ls) != 7:
    fail(f"seven fixtures in arch.inspect, got {len(ls)}")
at = [tuple(round(num(c, 'at'), 6) for c in l.get('at', [])) for l in ls[:4]]
if sorted(at) != [(1.0, 1.0), (1.0, 3.0), (3.0, 1.0), (3.0, 3.0)]:
    fail(f"the grid in the 4 x 4 m room sits at (1,1), (1,3), (3,1), (3,3): {at}")
for l in ls[:4]:
    near(l.get('z'), 2.4, 1e-9, f"{l.get('id')} light point (the room's ceiling)")
    near(l.get('candela'), 950.36, 0.05, f"{l.get('id')} candela (800 lm over a 60° cone)")
    if l.get('kind') != 'spot' or l.get('mount') != 'ceiling' or l.get('handle') != f"a00480{int(l['id'][2:]):02x}":
        fail(f"a pot light is a ceiling spot with its own handle: {l}")
heights = {l.get('fixture'): l.get('z') for l in ls[4:]}
near(heights.get('Pendant'), 1.8, 1e-9, 'pendant light point (0.6 m below the 2.4 m ceiling)')
near(heights.get('Wall sconce'), 1.8, 1e-9, 'wall sconce light point')
near(heights.get('Surface ceiling fixture'), 2.35, 1e-9, 'surface fixture light point')
print("ok: grid at the cell centres under the 2.4 m ceiling, 950.4 cd each; pendant, sconce and surface fixture at their heights")

b = probes(rr)
if num(b.get('lights'), 'build3d lights') != 7 or num(b.get('fixtures'), 'build3d fixtures') != 7:
    fail(f"build3d reports 7 lights and 7 fixture housings: lights {b.get('lights')}, fixtures {b.get('fixtures')}")
pr = b['probe']
if part(pr[0]) != 'light' or (pr[0] or {}).get('id') != 'lf0' or pr[1] is not None or part(pr[2]) != 'light':
    fail(f"the pot light's trim ring under the ceiling and the pendant's shade are solid 'light' parts, open air between: {pr}")
print("ok: build3d reports the lights; trim ring and pendant shade probe as 'light'")

rc, rr, so, se = run(room_cmds() + [POTS, cmd('lightfixture.edit', {"id": "lf0", "at": [1.5, 1.0], "lumens": 400, "on": False}),
                                    cmd('lightfixture.delete', {"id": "lf3"}), 'arch.inspect {"lights":true}', 'build3d {}'])
ls = lights_of(rr)
if [l.get('id') for l in ls] != ['lf0', 'lf1', 'lf2']:
    fail(f"after deleting lf3: lf0, lf1, lf2: {[l.get('id') for l in ls]}")
l0 = ls[0]
if [round(num(c, 'at'), 6) for c in l0['at']] != [1.5, 1.0] or num(l0.get('lumens'), 'lumens') != 400 or l0.get('on') is not False:
    fail(f"lf0 moved to (1.5, 1), 400 lm, off: {l0}")
b = reply_with(rr, 'fixtures')
if num(b.get('lights'), 'lights') != 2 or num(b.get('fixtures'), 'fixtures') != 3:
    fail(f"an off fixture keeps its housing and emits nothing: lights {b.get('lights')} (want 2), fixtures {b.get('fixtures')} (want 3)")
print("ok: edit moves, dims and switches off; delete removes; an off fixture keeps its housing")
print("PASS 01")
