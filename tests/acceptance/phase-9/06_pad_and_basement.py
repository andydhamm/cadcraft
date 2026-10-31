# The house's footprint flattens a building pad; a basement cuts the ground down inside its footprint.
# needs: p9-cmd-terrain, p9-cmd-terrain-query, p9-arch-terrain-pad, p9-arch-terrain-surface, p9-arch3d-terrain-solid
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

HOUSE = cmd('wall', {"points": [[15, 11], [25, 11], [25, 19], [15, 19], [15, 11]], "thickness": 0.2, "exterior": True})
Q = cmd('terrain.query', {"points": [[20, 15], [14.8, 15], [14.4, 15], [5, 15], [25.5, 15], [20, 15]]})
rc, rr, so, se = run([HOUSE, terrain_cmd(), Q])
e = elevations(rr)
want = [-0.3, -0.3, -0.2, 0.25, -0.3 + 0.5 * 0.1, -0.3]
what = ['(20,15) inside', '(14.8,15) on the 0.3 m apron', '(14.4,15) grading back at 1:2', '(5,15) beyond the grading',
        '(25.5,15) 0.4 m out, rising 1:2']
for got, w, t in zip(e[:5], want, what):
    near(got, w, 1e-6, f"pad: {t}")
print("ok: pad at -0.3 (level 0 less the 0.3 m structure), 0.3 m apron, 1:2 back to the ground")

rc, rr, so, se = run([HOUSE, terrain_cmd(pad=False), Q])
near(elevations(rr)[0], 1.0, 1e-6, 'pad off: (20,15) is the ground')

rc, rr, so, se = run([HOUSE, 'level {"below":0}',
                      cmd('wall', {"points": [[15, 11], [25, 11], [25, 19], [15, 19], [15, 11]], "thickness": 0.2, "exterior": True, "level": 0}),
                      terrain_cmd(), cmd('terrain.query', {"points": [[20, 15], [14.8, 15]]}),
                      cmd('build3d', {"probe": [[20, 15, -3.05], [20, 15, -2.95]]})])
e = elevations(rr)
near(e[0], -3.0, 1e-6, 'basement: (20,15) cut to the basement floor underside (-2.7 - 0.3)')
near(e[1], -0.3, 1e-6, 'basement: the first floor pad still holds on its apron')
p = probes(rr)['probe']
if part(p[0]) != 'terrain' or part(p[1]) == 'terrain':
    fail(f"the ground under the basement is at -3.0: probes {p}")
print("ok: basement cuts the ground to -3.0 inside its footprint")
print("PASS 06")
