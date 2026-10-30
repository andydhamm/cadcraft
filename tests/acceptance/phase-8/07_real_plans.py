# Corpus (c): three freely licensed real plans in tests/fixtures/archdetect/real/, each with its source and licence in
# ATTRIBUTION.md. Reported, not gated: detection runs on each without a panic and with a well-formed reply; when a
# <name>.truth.json sits beside a plan, precision and recall are printed. (The operator adds the plans: workers have no
# network.)
#
# Deferred (operator, 2026-10-30): the download of the three CC0 plans waits on Andy's approval. Until the operator adds
# them, an empty or missing real/ directory passes with a "deferred" line, so phase 9 can start; the open item stays in
# docs/arch3d/DESIGN.md. The check is strict again by itself the moment a plan lands: one or two plans fail (three are
# required), and every plan must be named in ATTRIBUTION.md and detect cleanly.
import glob, os, time
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())

plans = sorted(glob.glob('tests/fixtures/archdetect/real/*.dxf'))
if not plans:
    print("deferred 07: waiting on the operator's real-plan download (three CC0 plans; open item in docs/arch3d/DESIGN.md)")
    print("PASS 07 (deferred: no real plans yet)")
    raise SystemExit(0)
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
