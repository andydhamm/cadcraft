# Corpus (b): examples/apartment.dxf (millimetres, AIA layers, walls as face rectangles, doors as a leaf line and an ARC,
# windows as a rectangle with a centre line, dimensions, hatch, furniture, a table) against the hand-written truth in
# apartment.truth.json. Target: walls precision and recall >= 0.80; doors and windows are reported. The three rooms are
# found and named from the labels inside them ("LIVING / DINING", "BEDROOM", "BATH"). A dry run changes nothing.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())

A = 'examples/apartment.dxf'
truth_doc = json.load(open(os.path.join(HERE, 'apartment.truth.json')))
truth = {'walls': [{'id': w['id'], 'a': w['start'], 'b': w['end'], 't': w['thickness']} for w in truth_doc['walls']],
         'openings': [{'id': f"{o['kind']}@{o['center']}", 'kind': o['kind'], 'c': o['center'], 'w': o['width']} for o in truth_doc['openings']]}

_, rows, _, _ = run(['archdetect {"dryRun":true}', 'arch.inspect {}'], file=A)
r = dry_reply(rows)
if r.get('units') != 'mm' or abs(num(r.get('scale'), 'scale') - 0.001) > 1e-12:
    fail(f"apartment.dxf has $INSUNITS 4: units mm, scale 0.001 expected, got {r.get('units')}, {r.get('scale')}")
insp = rows[-1]
if insp.get('walls') or insp.get('revision', 0) != 0:
    fail(f"a dry run must not change the drawing: after it arch.inspect shows {json.dumps(insp)[:300]}")
if num((r.get('stats') or {}).get('entities'), 'stats.entities') < 58:
    fail(f"stats.entities must count the model-space entities read (58 in apartment.dxf): {r.get('stats')}")

T = Tally()
T.add('apartment', r, truth)
print(T.report())
wp, wr = T.pr('walls')
if wp < 0.80 or wr < 0.80:
    print('\n'.join(T.notes))
    fail(f"walls below 0.80 on apartment.dxf: precision {wp:.3f}, recall {wr:.3f}")

rooms = [x for x in r['rooms'] if num(x.get('confidence'), 'room confidence') >= MIN_CONF]
if len(rooms) != 3:
    fail(f"apartment.dxf has three rooms; {len(rooms)} room candidates at confidence >= {MIN_CONF}: "
         f"{[(x.get('name'), x.get('area')) for x in r['rooms']]}")
for want in truth_doc['rooms']:
    hit = [x for x in rooms if inside(want['label'], [pt(q, 'room polygon') for q in x.get('polygon') or []])]
    if len(hit) != 1:
        fail(f"exactly one room must contain the label point of {want['name']!r} {want['label']}: {len(hit)} do")
    if hit[0].get('name') != want['name']:
        fail(f"the room around {want['label']} must take its name from the label inside it, {want['name']!r} "
             f"(the MTEXT's first line, formatting removed), got {hit[0].get('name')!r}")
print(f"PASS 04: walls P {wp:.3f} R {wr:.3f}; rooms {[x['name'] for x in rooms]}")
