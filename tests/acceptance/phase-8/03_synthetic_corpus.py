# Corpus (a): plans built from our own models, saved, stripped of the arch record and xdata so only the proxy linework
# is left, detected, and compared with the model (truth = arch.inspect). Targets over the whole corpus: precision and
# recall >= 0.95 for walls and >= 0.90 for doors and for windows (candidates with confidence >= 0.5, the ones an accept of
# "all" creates). The corpus also reads through INSERTs (the house nested two blocks deep, turned 30 degrees and moved),
# honours $INSUNITS (the cabin inserted at scale 1000 into a millimetre drawing) and does not need layer names (the ell
# with every entity on layer 0).
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())

tmp = tempfile.mkdtemp()
T = Tally()
truths, plains = {}, {}
for name in PLANS:
    text, truth = build(name, tmp)
    truths[name], plains[name] = truth, plain(name, text, tmp)
    T.add(name, detect(plains[name]), truth)

p = wrapped('house-blocks', plains['house'], tmp, at=(100.0, 50.0), rotation=30.0)
T.add('house-blocks', detect(p), transform(truths['house'], 30.0, (100.0, 50.0)))

p = wrapped('cabin-mm', plains['cabin'], tmp, at=(0.0, 0.0), rotation=0.0, scale=1000.0, insunits=4)
r = detect(p)
if r.get('units') != 'mm' or abs(num(r.get('scale'), 'scale') - 0.001) > 1e-12:
    fail(f"a drawing with $INSUNITS 4 is in millimetres: the reply must say units mm, scale 0.001, got {r.get('units')}, {r.get('scale')}")
T.add('cabin-mm', r, truths['cabin'])

p = os.path.join(tmp, 'ell-no-layers.dxf')
open(p, 'w').write(relayer(open(plains['ell']).read(), '0'))
T.add('ell-no-layers', detect(p), truths['ell'])

a, b = detect(plains['house']), detect(plains['house'])
for k in ('walls', 'openings', 'rooms'):
    if a[k] != b[k]:
        fail(f"detection must be deterministic: two runs on the same plan differ in '{k}'")
st = a.get('stats') or {}
for k in ('entities', 'primitives', 'ms'):
    num(st.get(k), f'stats.{k}')

print(T.report())
bad = [k for k, lim in (('walls', 0.95), ('doors', 0.90), ('windows', 0.90)) if min(T.pr(k)) < lim]
if bad:
    print('\n'.join(T.notes[:60]))
    fail(f"below target on {', '.join(bad)} (walls >= 0.95, doors and windows >= 0.90): {T.report()}")
print("PASS 03")
