# ARCHDETECT {"accept": "all" | [ids]} creates the candidates as arch objects in ONE undo step. Accepting in a drawing
# that is not in metres first converts the drawing to metres in the same step (the arch model is in metres; decision
# 24), so the new walls sit on the linework they were found in. "all" creates exactly the candidates at confidence >= 0.5;
# a list creates exactly those; an opening without its wall, an unknown id or bad input changes nothing.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())

A = 'examples/apartment.dxf'
tmp = tempfile.mkdtemp()
out = os.path.join(tmp, 'accepted.dxf')
_, rows, _, _ = run(['archdetect {"dryRun":true}', 'archdetect {"accept":"all"}',
                     'arch.inspect {"walls":true,"openings":true,"rooms":true}', 'units {}'], file=A, save=out)
dry = dry_reply(rows)
acc = reply_with(rows, 'accepted', what='accept reply')
insp = [x for x in rows if isinstance(x, dict) and isinstance(x.get('walls'), list) and 'revision' in x][-1]
want = {k: [c['id'] for c in dry[k] if num(c.get('confidence'), 'confidence') >= MIN_CONF] for k in ('walls', 'openings', 'rooms')}
if sorted(acc['accepted']) != sorted(want['walls'] + want['openings'] + want['rooms']):
    fail(f"accept 'all' must create exactly the candidates at confidence >= {MIN_CONF}: accepted {sorted(acc['accepted'])}, expected {want}")
for k in ('walls', 'openings', 'rooms'):
    if len(insp.get(k) or []) != len(want[k]):
        fail(f"after accepting, arch.inspect lists {len(insp.get(k) or [])} {k}, the dry run offered {len(want[k])}")
conv = acc.get('converted') or {}
if conv.get('from') != 'mm' or abs(num(conv.get('scale'), 'converted.scale') - 0.001) > 1e-12:
    fail(f"accepting in a millimetre drawing must convert it to metres and say so ({{'from':'mm','scale':0.001}}), got {acc.get('converted')}")
if rows[-1].get('insunits') != 6:
    fail(f"after the conversion $INSUNITS must be 6 (metres), got {rows[-1]}")
names = sorted(x.get('name') for x in insp['rooms'])
if names != sorted(x['name'] for x in json.load(open(os.path.join(HERE, 'apartment.truth.json')))['rooms']):
    fail(f"the accepted rooms must carry the label names: {names}")

# The linework moved with the conversion: the outer wall face rectangle is now 0..12 x 0..8 (metres).
p = pairs_of(open(out).read())
rings = []
for name, s, e in sections(p):
    if name == 'ENTITIES':
        cur = None
        for k in range(s, e):
            c, v = p[k]
            if c == '0':
                cur = {'type': v.strip(), 'layer': '', 'xy': [], 'own': False}; rings.append(cur)
            elif cur is None:
                continue
            elif c == '8':
                cur['layer'] = v.strip()
            elif c == '10':
                cur['xy'].append([float(v), None])
            elif c == '20' and cur['xy']:
                cur['xy'][-1][1] = float(v)
            elif c == '1001' and v.strip() == 'CADCRAFT':
                cur['own'] = True
lw = [x for x in rings if x['type'] == 'LWPOLYLINE' and x['layer'] == 'A-WALL' and not x['own'] and x['xy']]
xs = [q[0] for x in lw for q in x['xy']]; ys = [q[1] for x in lw for q in x['xy']]
if not xs or abs(min(xs)) > 1e-6 or abs(max(xs) - 12.0) > 1e-6 or abs(min(ys)) > 1e-6 or abs(max(ys) - 8.0) > 1e-6:
    fail(f"the A-WALL linework must be converted to metres with the drawing (0..12 x 0..8), got x {min(xs or [0])}..{max(xs or [0])}, y {min(ys or [0])}..{max(ys or [0])}")
print(f"ok: accepted {len(acc['accepted'])} candidates, converted mm -> m, linework 0..12 x 0..8")

# One undo step takes all of it back, the conversion included.
_, r2, _, _ = run(['archdetect {"accept":"all"}', 'undo {}', 'arch.inspect {"walls":true}', 'units {}'], file=A)
und = reply_with(r2, 'undone', what='undo')
if len(und['undone']) != 1:
    fail(f"accept must be one undo step: undo took back {und['undone']}")
if (reply_list(r2, 'walls', what='inspect after undo')['walls']) or r2[-1].get('insunits') != 4:
    fail(f"one undo must remove every accepted object and the unit conversion: {json.dumps(r2[-2:])[:400]}")
print("ok: one undo step")

# A list accepts exactly those; an opening without its wall, an unknown id, or a bad value changes nothing.
w0 = want['walls'][0]
_, r3, _, _ = run([f'archdetect {{"accept":["{w0}"]}}', 'arch.inspect {"walls":true}'], file=A)
if len(reply_list(r3, 'walls', what='inspect')['walls']) != 1:
    fail(f"accept [{w0}] must create exactly one wall")
op = next((o for o in dry['openings'] if o.get('host')), None) or fail("the dry run offered no opening with a host")
for bad in ([op['id']], ['c999999'], 'some', [1, 2], {}):
    rc, r4, so, se = run([f'archdetect {json.dumps({"accept": bad})}', 'arch.inspect {"walls":true,"openings":true}', 'units {}'], file=A, expect_ok=False)
    last = reply_list(r4, 'walls', what='inspect after a refused accept')
    if last['walls'] or last.get('openings') or last.get('revision', 0) != 0 or r4[-1].get('insunits') != 4:
        fail(f"accept {bad!r} must be refused and change nothing (no walls, no openings, still millimetres): {json.dumps(r4)[:400]}")
    if 'error' not in (so + se).lower():
        fail(f"accept {bad!r} must say why it was refused: {(so + se)[-300:]}")

# In a metre drawing nothing is converted.
text, _ = build('house', tmp)
pp = plain('house', text, tmp)
_, r5, _, _ = run(['archdetect {"accept":"all"}'], file=pp)
if reply_with(r5, 'accepted', what='accept in metres').get('converted') is not None:
    fail("a drawing already in metres is not converted (converted must be null)")
print("PASS 05")
