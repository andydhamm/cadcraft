# Moving a window by 1 m moves its rectangle in the elevation by 1 m with no other command: the view regenerates
# when arch_revision changes. Undo brings the old geometry back (the view lives in the Drawing).
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
WIN = 'window {"wall":"w0","offset":6.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}'
_, rows, _, _ = run(HOUSE + [WIN] + ROOF + [
    'elevation {"side":"south","insert":[0,-20]}', 'arch.inspect {"views":true}',
    'arch.opening.move {"opening":"o1","offset":7.0}', 'arch.inspect {"views":true}',
    'u {}', 'arch.inspect {"views":true}'])
def span(nth, ident):
    L = [l for l in lines_of(views_of(rows, nth)[0]) if l['kind'] == 'opening' and l['id'] == ident]
    if not L:
        fail(f"no lines of {ident} in inspect #{nth + 1}")
    us = [p[0] for l in L for p in (l['a'], l['b'])]
    return min(us), max(us), sorted((round(l['a'][0], 6), round(l['a'][1], 6), round(l['b'][0], 6), round(l['b'][1], 6)) for l in L)
b0, b1, _ = span(0, 'o1')
a0, a1, _ = span(1, 'o1')
if abs(a0 - b0 - 1.0) > 1e-6 or abs(a1 - b1 - 1.0) > 1e-6:
    fail(f"window o1 spans u {b0:.4f}..{b1:.4f} before and {a0:.4f}..{a1:.4f} after a 1 m move")
if span(0, 'o0')[2] != span(1, 'o0')[2]:
    fail("moving the window moved the door's lines")
revs = [reply_with(rows, 'views', i, 'arch.inspect') for i in (0, 1)]
if views_of(rows, 1)[0].get('revision') != revs[1].get('revision') or revs[1].get('revision') == revs[0].get('revision'):
    fail(f"the view was not regenerated at the new revision: {views_of(rows, 1)[0].get('revision')} vs {revs}")
u0, u1, _ = span(2, 'o1')
if abs(u0 - b0) > 1e-6 or abs(u1 - b1) > 1e-6:
    fail(f"undo left the window at u {u0:.4f}..{u1:.4f}, want {b0:.4f}..{b1:.4f}")
print(f"ok: window u {b0:.3f}..{b1:.3f} -> {a0:.3f}..{a1:.3f} after a 1 m move, back after undo")
