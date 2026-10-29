# The front (south) elevation of the phase-1 house with its 6/12 gable roof (0.5 m overhang) and the front door at
# offset 3.0: a linked ELEVATION view in the arch model, reported by arch.inspect {"views":true} in view coordinates
# (u right, v up, metres, as arch3d::hlr::OrthoView projects). Outline 11.2 m wide; the door a closed 0.9 x 2.1
# rectangle; placed with its lower-left at `insert`; an overall width and height dimension.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
_, rows, _, _ = run(HOUSE + ROOF + ['elevation {"side":"south","insert":[0,-20]}', 'arch.inspect {"views":true}'])
made = reply_with(rows, 'view', what='elevation reply')
if made.get('view') != 'v0' or made.get('handle') != 'a0030000':
    fail(f"elevation must reply with the new view's id v0: {made}")
vs = views_of(rows)
if len(vs) != 1:
    fail(f"one view expected, got {len(vs)}")
v = vs[0]
if v.get('kind') != 'elevation' or v.get('layer') != 'A-ELEV':
    fail(f"kind/layer: {v.get('kind')!r} {v.get('layer')!r}, want 'elevation' on 'A-ELEV'")
look = [num(x, 'look') for x in v.get('look') or []]
if len(look) != 2 or abs(look[0]) > 1e-9 or abs(look[1] - 1) > 1e-9:
    fail(f"the south elevation looks north (+y): look {v.get('look')}")
if v.get('handle') != 'a0030000':
    fail(f"view handle {v.get('handle')!r}, want 'a0030000' (views take handle offset 0x30000)")
ins = reply_with(rows, 'views', what='arch.inspect')
if v.get('revision') != ins.get('revision'):
    fail(f"view generated at revision {v.get('revision')}, model is at {ins.get('revision')}: stale")
L = lines_of(v)
if len(L) < 20:
    fail(f"only {len(L)} lines in the front elevation")
kinds = {l['kind'] for l in L}
if not {'wall', 'opening', 'roof'} <= kinds:
    fail(f"the front elevation shows walls, the door and the roof: kinds {sorted(kinds)}")
e = extent(L)
if abs(e[2] - e[0] - 11.2) > 0.005:
    fail(f"front elevation outline is {e[2] - e[0]:.4f} m wide, want 11.2 (10.2 m of wall + two 0.5 m overhangs)")
rep = [num(x, 'extent') for x in v.get('extent') or []]
if len(rep) != 4 or any(abs(a - b) > 1e-6 for a, b in zip(rep, e)):
    fail(f"extent {v.get('extent')} is not the bounds of the lines {e}")
x = closed_rect(L, 0.9, 2.1)
if x is None:
    fail("no closed 0.9 x 2.1 m door rectangle (two 2.1 m jambs 0.9 m apart joined by head and sill)")
if not any(l['kind'] == 'opening' and l['id'] == 'o0' for l in L):
    fail("no line belongs to the door o0")
if any(l['kind'] == 'opening' and l['id'] != 'o0' for l in L):
    fail("a line names an opening that does not exist")
ins_pt = pt(v.get('insert'), 'insert')
b = [num(q, 'bounds') for q in v.get('bounds') or []]
want_b = [ins_pt[0], ins_pt[1], ins_pt[0] + e[2] - e[0], ins_pt[1] + e[3] - e[1]]
if len(b) != 4 or any(abs(p - q) > 1e-6 for p, q in zip(b, want_b)):
    fail(f"placed bounds {v.get('bounds')}, want {want_b} (lower-left of the extent at insert {ins_pt})")
dims = {d.get('kind'): d for d in v.get('dims') or [] if isinstance(d, dict)}
if set(dims) != {'width', 'height'} or len(v.get('dims')) != 2:
    fail(f"one overall width and one overall height dimension: {v.get('dims')}")
if abs(num(dims['width'].get('value'), 'width dim') - 11.2) > 0.005 or dims['width'].get('text') != '11200':
    fail(f"width dimension {dims['width']}, want 11.2 m shown as '11200'")
if abs(num(dims['height'].get('value'), 'height dim') - (e[3] - e[1])) > 1e-6:
    fail(f"height dimension {dims['height'].get('value')} is not the outline height {e[3] - e[1]:.4f}")
print(f"ok: {len(L)} lines, outline {e[2] - e[0]:.3f} x {e[3] - e[1]:.3f} m, door rectangle at u={x:.3f}, dims 11200 / {dims['height'].get('text')}")
