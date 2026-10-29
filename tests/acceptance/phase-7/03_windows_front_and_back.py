# A window put only on the back wall adds no line to the front elevation; the same window on the front wall does.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
WIN = 'window {"wall":"%s","offset":%s,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}'
def front(extra):
    _, rows, _, _ = run(HOUSE + extra + ROOF + ['elevation {"side":"south","insert":[0,-20]}', 'arch.inspect {"views":true}'])
    return lines_of(views_of(rows)[0])
key = lambda L: sorted((l['kind'], l['id'], round(l['a'][0], 6), round(l['a'][1], 6), round(l['b'][0], 6), round(l['b'][1], 6)) for l in L)
base = front([])
back = front([WIN % ('w2', 4.0)])
fr = front([WIN % ('w0', 6.0)])
if key(back) != key(base):
    fail(f"a window on the back wall changed the front elevation: {len(base)} lines -> {len(back)}")
if any(l['id'] == 'o1' for l in back):
    fail("the back window o1 shows in the front elevation")
if len(fr) <= len(base):
    fail(f"a window on the front wall added no line: {len(base)} -> {len(fr)}")
if not any(l['kind'] == 'opening' and l['id'] == 'o1' for l in fr):
    fail("no line of the front window o1 in the front elevation")
print(f"ok: base {len(base)} lines, back window {len(back)}, front window {len(fr)}")
