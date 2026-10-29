# A section at y = 4 looking +y: one filled ring per cut wall, of area thickness x height (0.2 x 2.4: the east and
# west walls are eave walls and keep their 2.4 m under the overhanging roof), plus poché for the slab and the roof.
# The door is behind the cut (south wall), so none of its lines shows.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
_, rows, _, _ = run(HOUSE + ROOF + ['section {"from":[-1,4],"to":[11,4],"look":[0,1],"insert":[0,-40]}', 'arch.inspect {"views":true}'])
v = views_of(rows)[0]
if v.get('kind') != 'section' or v.get('layer') != 'A-SECT':
    fail(f"kind/layer {v.get('kind')!r} {v.get('layer')!r}, want 'section' on 'A-SECT'")
cut = v.get('cut')
if not isinstance(cut, list) or len(cut) != 2 or pt(cut[0], 'cut') != [-1.0, 4.0] or pt(cut[1], 'cut') != [11.0, 4.0]:
    fail(f"cut line {cut}, want [[-1,4],[11,4]]")
rings = []
for i, r in enumerate(v.get('rings') or []):
    pts = [pt(p, f'ring {i}') for p in (r.get('points') or [])]
    if len(pts) < 3:
        fail(f"ring {i} is not a ring: {r!r:.200}")
    area = abs(shoelace(pts))
    if abs(num(r.get('area'), f'ring {i} area') - area) > 1e-6:
        fail(f"ring {i} reports area {r.get('area')} but its points enclose {area:.6f}")
    rings.append({'kind': r.get('kind'), 'id': r.get('id'), 'area': area, 'points': pts})
walls = [r for r in rings if r['kind'] == 'wall']
if sorted(r['id'] for r in walls) != ['w1', 'w3']:
    fail(f"the cut walls are w1 and w3 (east, west), one ring each: {[(r['id'], round(r['area'], 4)) for r in walls]}")
for r in walls:
    if abs(r['area'] - 0.2 * 2.4) > 0.002:
        fail(f"wall {r['id']} poché is {r['area']:.4f} m², want 0.2 x 2.4 = 0.48 (wall raised into the roof?)")
if not any(r['kind'] == 'room' for r in rings):
    fail(f"no slab poché (a ring of kind 'room'): {[(r['kind'], r['id']) for r in rings]}")
if not any(r['kind'] == 'roof' for r in rings):
    fail(f"no roof poché (a ring of kind 'roof'): {[(r['kind'], r['id']) for r in rings]}")
L = lines_of(v)
if any(l['kind'] == 'opening' and l['id'] == 'o0' for l in L):
    fail("the door o0 is behind the cut but shows in the section")
print(f"ok: wall rings {[(r['id'], round(r['area'], 4)) for r in walls]}, {len(rings)} rings, {len(L)} lines beyond")
