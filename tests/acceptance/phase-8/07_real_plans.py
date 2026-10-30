# Corpus (c): three freely licensed real plans in tests/fixtures/archdetect/real/, each with its source and licence in
# ATTRIBUTION.md. Reported, not gated: detection runs on each without a panic and with a well-formed reply; when a
# <name>.truth.json sits beside a plan, precision and recall are printed. (The operator adds the plans: workers have no
# network.)
import glob, os, time
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())

plans = sorted(glob.glob('tests/fixtures/archdetect/real/*.dxf'))
if len(plans) < 3:
    fail(f"tests/fixtures/archdetect/real/ must hold three freely licensed real plans, found {plans}")
attr = open('ATTRIBUTION.md', encoding='utf-8').read()
for p in plans:
    if os.path.basename(p) not in attr:
        fail(f"ATTRIBUTION.md must name {os.path.basename(p)} with its source and licence")
for p in plans:
    t0 = time.time()
    r = detect(p)
    walls, ops = candidates(r)
    line = (f"{os.path.basename(p)}: {len(walls)} walls, {sum(o['kind'] == 'door' for o in ops)} doors, "
            f"{sum(o['kind'] == 'window' for o in ops)} windows, {len([x for x in r['rooms'] if x.get('confidence', 0) >= MIN_CONF])} rooms "
            f"({r.get('units')}, {(r.get('stats') or {}).get('entities')} entities, {time.time() - t0:.1f} s)")
    tp = p[:-4] + '.truth.json'
    if os.path.isfile(tp):
        d = json.load(open(tp))
        T = Tally()
        T.add(os.path.basename(p), r, {'walls': [{'id': w.get('id', ''), 'a': w['start'], 'b': w['end'], 't': w['thickness']} for w in d['walls']],
                                       'openings': [{'id': '', 'kind': o['kind'], 'c': o['center'], 'w': o['width']} for o in d.get('openings', [])]})
        line += ' — ' + T.report()
    print(line)
print("PASS 07 (reported, not gated)")
