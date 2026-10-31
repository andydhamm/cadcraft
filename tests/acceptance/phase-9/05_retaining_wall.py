# A retaining wall holds a flat region at 2.5 m against ground at 0.05 x.
# needs: p9-cmd-terrain, p9-cmd-terrain-modify, p9-cmd-terrain-query, p9-cmd-wall-retaining, p9-arch-retaining, p9-cmd-site-inspect
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

EAST = [[30, 0], [40, 0], [40, 30], [30, 30]]
WALL = cmd('wall', {"points": [[30, 0], [30, 30]], "thickness": 0.2, "retaining": True, "exterior": False})
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"flat": {"polygon": EAST, "elevation": 2.5, "margin": 0}}), WALL,
                      cmd('terrain.query', {"points": [[30.01, 15], [29.99, 15]]}), 'arch.inspect {"walls":true}',
                      cmd('build3d', {"probe": [[30, 15, 2.6], [30, 15, 2.7], [30.05, 15, 2.55], [35, 15, 2.45], [35, 15, 2.55], [25, 15, 1.2]]})])
e = elevations(rr)
near(e[0], 2.5, 1e-6, 'elevation at (30.01,15) on the high side')
near(e[1], 1.4995, 1e-6, 'elevation at (29.99,15) on the low side')
walls = reply_with(rr, 'walls', -1)['walls']
w = walls[0] if walls else {}
if w.get('retaining') is not True:
    fail(f"arch.inspect must report the wall as retaining: {w}")
near(w.get('top'), 2.65, 1e-6, 'retaining wall top (high side 2.5 + 0.15)')
near(w.get('bottom', w.get('base')), 1.4945 - 0.3, 1e-6, 'retaining wall bottom (low side 1.4945 at the face + 0.01, less the 0.3 m footing)')
p = probes(rr)['probe']
if [part(h) for h in p] != ['wall', None, 'wall', 'terrain', None, 'terrain']:
    fail(f"wall from 1.1945 to 2.65 at x = 30, flat ground at 2.5 east, sloped ground west: probes {p}")
print("ok: (30.01,15) 2.5, (29.99,15) 1.4995, wall top 2.65, bottom 1.1945")

# Freeboard and footing are settable.
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"flat": {"polygon": EAST, "elevation": 2.5, "margin": 0}}),
                      cmd('wall', {"points": [[30, 0], [30, 30]], "thickness": 0.2, "retaining": True, "exterior": False, "freeboard": 0.3, "footing": 0.5}),
                      'arch.inspect {"walls":true}'])
w = reply_with(rr, 'walls', -1)['walls'][0]
near(w.get('top'), 2.8, 1e-6, 'top with freeboard 0.3'); near(w.get('base'), 1.4945 - 0.5, 1e-6, 'bottom with footing 0.5')
rc, rr, so, se = run([cmd('wall', {"points": [[0, 0], [5, 0]], "thickness": 0.2, "freeboard": 0.3}), 'arch.inspect {"walls":true}'], expect_ok=False)
if not was_refused(rc, se) or reply_with(rr, 'walls', -1)['walls']:
    fail("freeboard on a wall that is not retaining must be refused, nothing written")
print("ok: freeboard and footing settable; refused on an ordinary wall")
print("PASS 05")
