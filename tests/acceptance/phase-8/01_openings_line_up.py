# An opening's offset is the distance from its wall's start to its CENTRE, everywhere (decision 24): the plan gap
# (rough span), the plan symbol, the 3D hole and the placement limits agree. Before phase 8 the 3D hole and the symbol's
# leaf were centred while the poché gap, the jambs and the end limits read the offset as the start, so every gap sat half
# a width off its door. Also: the elevation draws no header rectangle above an opening (phase-7 gap).
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())

W = 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
_, rows, _, _ = run([W, 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}',
                     'window {"wall":"w0","offset":7.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}',
                     'arch.inspect {"openings":true,"symbols":true}',
                     'build3d {"probe":[[3.0,0.0,1.0],[2.52,0.0,1.0],[3.48,0.0,1.0],[2.7,0.0,1.0],[3.3,0.0,1.0]]}'])
ops = {o['id']: o for o in reply_list(rows, 'openings', what='arch.inspect openings')['openings']}
for oid, centre, rough in (('o0', 3.0, 1.02), ('o1', 7.0, 1.32)):
    o = ops.get(oid) or fail(f"no opening {oid}: {list(ops)}")
    gap = [num(x, f'{oid} gap') for x in (o.get('gap') or [])]
    if len(gap) != 2 or abs(gap[0] - (centre - rough / 2)) > 1e-6 or abs(gap[1] - (centre + rough / 2)) > 1e-6:
        fail(f"{oid} at offset {centre}: the poché gap must be the rough span centred on the offset, "
             f"[{centre - rough / 2:.3f}, {centre + rough / 2:.3f}], got {gap}")
    xs = [num(p[0], 'symbol x') for pl in o.get('symbol') or [] for p in pl]
    if not xs or abs(min(xs) - gap[0]) > 1e-6 or abs(max(xs) - gap[1]) > 1e-6:
        fail(f"{oid}: the plan symbol must span exactly the gap {gap} along the wall, got x {min(xs or [0]):.3f}..{max(xs or [0]):.3f}")
print("ok: gaps and symbols centred on the offset")

pr = reply_with(rows, 'probe', what='build3d probe')['probe']
if len(pr) != 5:
    fail(f"five probes, got {pr}")
part = lambda h: (h or {}).get('part')
if part(pr[0]) == 'wall' or part(pr[3]) == 'wall' or part(pr[4]) == 'wall':
    fail(f"inside the door (x 2.7..3.3 at offset 3.0, width 0.9) the 3D wall must be cut: {pr[0]}, {pr[3]}, {pr[4]}")
if part(pr[1]) == 'wall' or part(pr[2]) == 'wall':
    fail(f"the 3D hole must be the rough span 2.49..3.51 (x 2.52 and 3.48 are inside it): {pr[1]}, {pr[2]}")
_, r2, _, _ = run([W, 'door {"wall":"w0","offset":3.0,"width":0.9}', 'build3d {"probe":[[2.45,0.0,1.0],[3.55,0.0,1.0]]}'])
after = reply_with(r2, 'probe', what='probe after the door')['probe']
if part(after[0]) != 'wall' or part(after[1]) != 'wall':
    fail(f"just outside the rough span 2.49..3.51 (x 2.45 and 3.55) the wall stays: {after}")
print("ok: the 3D hole is the same span")

# Placement limits read the same centre: the rough span must stay 0.15 m inside each end of the 10 m wall.
for off, ok in ((0.5, False), (0.8, True), (9.3, True), (9.5, False)):
    rc, rr, out, err = run([W, f'door {{"wall":"w0","offset":{off},"width":0.9}}', 'arch.inspect {"openings":true}'], expect_ok=False)
    have = [r for r in rr if isinstance(r, dict) and isinstance(r.get('openings'), list) and isinstance(r.get('walls'), list)]
    n = len(have[-1]['openings']) if have else 0
    if (n == 1) != ok:
        fail(f"a 0.9 m door centred at {off} on a 10 m wall must be {'accepted' if ok else 'refused'} "
             f"(rough span {off - 0.51:.2f}..{off + 0.51:.2f}, ends 0.15 m from the wall ends): {(err or out)[-300:]}")
print("ok: placement limits measure the centred rough span")

# Elevation: the door is drawn up to its rough top (2.22) and nothing of it above (no header rectangle).
_, r3, _, _ = run([W, 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1}', 'elevation {"side":"south"}', 'arch.inspect {"views":true}'])
views = reply_list(r3, 'views', what='arch.inspect views')['views']
if not views:
    fail("no elevation view")
ls = [l for l in views[0].get('lines') or [] if isinstance(l, dict) and l.get('kind') == 'opening']
if not ls:
    fail("the elevation draws no lines for the door")
vs = [num(p[1], 'v') for l in ls for p in (l['a'], l['b'])]
v0 = min(vs)
if max(vs) > v0 + 2.22 + 1e-3:
    tall = [l for l in ls if max(l['a'][1], l['b'][1]) > v0 + 2.221]
    fail(f"the elevation draws the door {max(vs) - v0:.3f} m tall; nothing of it may show above its rough top (2.22): the "
         f"header box is flush with the wall faces, so its outline shows. Lines above: {tall[:4]}")
print(f"PASS 01: one centre for gap, symbol, 3D hole and limits; door drawn {max(vs) - v0:.3f} m tall in elevation")
