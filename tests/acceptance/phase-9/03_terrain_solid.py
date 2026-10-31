# The terrain is a closed solid: the surface, a skirt, and a flat bottom 1 m below its lowest point.
# needs: p9-cmd-terrain, p9-cmd-terrain-modify, p9-arch-terrain-mesh, p9-arch3d-terrain-solid, p9-io-site-proxies
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

tmp = tempfile.mkdtemp()
dxf = os.path.join(tmp, 'site.dxf'); out = os.path.join(tmp, 'site.glb')
rc, rr, so, se = run([terrain_cmd(), cmd('build3d', {"probe": [[25, 12, 1.2], [25, 12, 1.3], [5, 5, -0.95], [5, 5, -1.05], [41, 5, 0.5], [39.9, 29.9, 1.9]]})],
                     save=dxf)
b = probes(rr)
if num(b.get('terrain'), 'build3d terrain') != 1:
    fail(f"build3d must report terrain: 1, got {b.get('terrain')}")
p = b['probe']
want = ['terrain', None, 'terrain', None, None, 'terrain']
for i, (hit, w) in enumerate(zip(p, want)):
    if part(hit) != w:
        fail(f"probe {i} {[[25,12,1.2],[25,12,1.3],[5,5,-0.95],[5,5,-1.05],[41,5,0.5],[39.9,29.9,1.9]][i]}: expected {w}, got {hit}")
print("ok: probes find the ground under the surface and above the bottom at -1.0, nothing above it or outside")
cli('convert', [dxf, out, '--no-detect'])
j, binc = glb(out)
lo, hi = node_box(j, 'terrain')
for k, (a, bb) in enumerate(zip([0, 0, -1.0], [40, 30, 2.0])):
    if abs(lo[k] - a) > 1e-3 or abs(hi[k] - bb) > 1e-3:
        fail(f"terrain node box axis {k}: expected {a}..{bb}, got {lo[k]}..{hi[k]}")
if not closed(j, binc, 'terrain'):
    fail("the terrain mesh must be closed: every edge used by exactly two triangles")
print("ok: GLB node 'terrain' 40 x 30, from -1.0 to 2.0, closed")

# A flat region with margin 0 is a step; the solid stays closed across it.
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.modify', {"flat": {"polygon": [[15, 10], [25, 10], [25, 20], [15, 20]], "elevation": 1.0}}),
                      cmd('build3d', {"probe": [[16, 15, 0.95], [16, 15, 1.05], [14, 15, 0.65], [14, 15, 0.75]]})], save=dxf)
p = probes(rr)['probe']
if [part(h) for h in p] != ['terrain', None, 'terrain', None]:
    fail(f"flat region at 1.0 next to ground at 0.7: probes {p}")
cli('convert', [dxf, out, '--no-detect'])
j, binc = glb(out)
if not closed(j, binc, 'terrain'):
    fail("the terrain stays closed across a step (vertical faces fill it)")
print("ok: a step keeps the solid closed")
print("PASS 03")
