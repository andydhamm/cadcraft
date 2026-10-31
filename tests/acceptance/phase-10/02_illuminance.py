# Direct illuminance from the pot lights, with the brief's numbers: 165.0 lx under a light, 122.1 lx inside the cone edge,
# 0 behind a column.
# needs: p10-arch-light-placement, p10-arch3d-scene-lights, p10-cmd-lightfixture, p10-cmd-illuminance
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())


def lux(rows, nth):
    hits = [r for r in rows if isinstance(r, dict) and 'lux' in r and 'contributions' in r]
    if len(hits) <= nth:
        fail(f"no illuminance reply #{nth}: {json.dumps(rows)[:600]}")
    return hits[nth]


q = lambda p: cmd('illuminance', {"point": p})
rc, rr, so, se = run(room_cmds() + [POTS, 'build3d {}', q([1, 1, 0]), q([1.8, 1.8, 0]), q([2, 2, 0]),
                                    cmd('illuminance', {"point": [1, 1, 0], "normal": [0, 0, -1]})])
if num(reply_with(rr, 'fixtures').get('lights'), 'lights') != 4:
    fail("the scene reports four lights")
a = lux(rr, 0)
near(a['lux'], 165.0, 1.65, 'illuminance at (1,1,0) (950.4 cd / 2.4² m²)')
on = [c for c in a['contributions'] if num(c.get('lux'), 'lux') > 1e-6]
if [c.get('id') for c in on] != ['lf0'] or num(a.get('lights'), 'lights') != 4:
    fail(f"only lf0 lights (1,1,0); the other three are outside their cones: {a}")
near(lux(rr, 1)['lux'], 122.1, 1.221, 'illuminance at (1.8,1.8,0)')
near(lux(rr, 2)['lux'], 0.0, 1e-6, 'illuminance at (2,2,0), outside every 30° half-cone')
near(lux(rr, 3)['lux'], 0.0, 1e-6, 'illuminance on a face turned away from the lights')
if lux(rr, 0).get('direct') is not True:
    fail("illuminance reports direct light only (direct: true)")
print("ok: 165.0 lx under lf0, 122.1 lx at (1.8,1.8), 0 at (2,2) and on a downward face")

rc, rr, so, se = run(room_cmds() + [POTS, COLUMN, q([1.8, 1.8, 0]), q([1, 1, 0])])
near(lux(rr, 0)['lux'], 0.0, 0.5, 'illuminance at (1.8,1.8,0) behind the 0.2 x 0.2 x 1.0 m column')
near(lux(rr, 1)['lux'], 165.0, 1.65, 'illuminance at (1,1,0) with the column')
print("ok: the column shadows (1.8,1.8,0) (0 lx) and leaves (1,1,0) at 165 lx")

rc, rr, so, se = run(room_cmds() + [q([1, 1, 0])])
if num(lux(rr, 0)['lux'], 'lux') != 0 or lux(rr, 0)['contributions']:
    fail("no fixtures, no light")
print("PASS 02")
