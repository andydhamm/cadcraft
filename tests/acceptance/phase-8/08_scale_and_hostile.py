# A plan of more than 10,000 entities is detected in under 2 s (median of 5 dry runs, `stats.ms` measured inside the
# command, release build), and finds every tile's walls. Hostile linework (random entities, huge and degenerate
# coordinates, self- and mutually-recursive blocks, deep nesting, a huge MINSERT array, odd $INSUNITS) and hostile
# parameters never panic, and an accept that fails leaves no half-applied edit.
import os, statistics
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())
random.seed(8)


def dxf(ents, insunits=4, blocks=''):
    h = f"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{insunits}\n0\nENDSEC\n"
    b = f"0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n" if blocks else ''
    return h + b + "0\nSECTION\n2\nENTITIES\n" + ''.join(ents) + "0\nENDSEC\n0\nEOF\n"


def line(a, b, layer='0'):
    return f"0\nLINE\n8\n{layer}\n10\n{a[0]}\n20\n{a[1]}\n30\n0\n11\n{b[0]}\n21\n{b[1]}\n31\n0\n"


def lw(pts, layer='0', closed=1):
    return f"0\nLWPOLYLINE\n8\n{layer}\n90\n{len(pts)}\n70\n{closed}\n" + ''.join(f"10\n{x}\n20\n{y}\n" for x, y in pts)


def arc(c, r, a0, a1, layer='0'):
    return f"0\nARC\n8\n{layer}\n10\n{c[0]}\n20\n{c[1]}\n30\n0\n40\n{r}\n50\n{a0}\n51\n{a1}\n"


def circle(c, r, layer='0'):
    return f"0\nCIRCLE\n8\n{layer}\n10\n{c[0]}\n20\n{c[1]}\n30\n0\n40\n{r}\n"


def ins(name, at, rot=0, s=1, cols=1, rows=1, layer='0'):
    return (f"0\nINSERT\n8\n{layer}\n2\n{name}\n10\n{at[0]}\n20\n{at[1]}\n30\n0\n41\n{s}\n42\n{s}\n43\n{s}\n50\n{rot}\n"
            + (f"70\n{cols}\n71\n{rows}\n44\n10\n45\n10\n" if cols > 1 or rows > 1 else ''))


def block(name, ents):
    return f"0\nBLOCK\n8\n0\n2\n{name}\n70\n0\n10\n0\n20\n0\n30\n0\n3\n{name}\n" + ''.join(ents) + "0\nENDBLK\n8\n0\n"


def tile(ox, oy):
    """The apartment's walls, doors and windows in millimetres (as examples/apartment.dxf draws them), plus furniture."""
    R = lambda x0, y0, x1, y1, L: lw([(ox + x0, oy + y0), (ox + x1, oy + y0), (ox + x1, oy + y1), (ox + x0, oy + y1)], L)
    e = [R(0, 0, 12000, 8000, 'A-WALL'), R(200, 200, 11800, 7800, 'A-WALL'), R(7000, 200, 7100, 1500, 'A-WALL'),
         R(7000, 2400, 7100, 7800, 'A-WALL'), R(7100, 5000, 9000, 5100, 'A-WALL'), R(9800, 5000, 11800, 5100, 'A-WALL'),
         line((ox + 7100, oy + 1500), (ox + 8000, oy + 1500), 'A-DOOR'), arc((ox + 7100, oy + 1500), 900, 0, 90, 'A-DOOR'),
         line((ox + 9000, oy + 5000), (ox + 9000, oy + 4200), 'A-DOOR'), arc((ox + 9000, oy + 5000), 800, 270, 0, 'A-DOOR')]
    for x0, y0, x1, y1 in ((1500, 0, 3500, 200), (4200, 0, 6200, 200), (8600, 0, 10600, 200)):
        e += [R(x0, y0, x1, y1, 'A-GLAZ'), line((ox + x0, oy + 100), (ox + x1, oy + 100), 'A-GLAZ')]
    e += [R(0, 4300, 200, 6300, 'A-GLAZ'), line((ox + 100, oy + 4300), (ox + 100, oy + 6300), 'A-GLAZ'),
          R(11800, 5800, 12000, 7200, 'A-GLAZ'), line((ox + 11900, oy + 5800), (ox + 11900, oy + 7200), 'A-GLAZ')]
    for i in range(11):
        x, y = 600 + 550 * (i % 6), 3000 + 1500 * (i // 6)
        e.append(R(x, y, x + 450, y + 450, 'A-FURN'))
    for i in range(8):
        e.append(circle((ox + 1000 + 500 * i, oy + 6800), 200, 'A-FURN'))
    return e


tmp = tempfile.mkdtemp()
N = 17
ents = [x for i in range(N) for j in range(N) for x in tile(i * 13000, j * 10000)]
if len(ents) < 10000:
    fail(f"the scale plan needs >= 10,000 entities, built {len(ents)}")
big = os.path.join(tmp, 'big.dxf')
open(big, 'w').write(dxf(ents))
_, rows, _, _ = run(['archdetect {"dryRun":true}'] * 5, file=big, release=True, timeout=1800)
reps = [r for r in rows if isinstance(r, dict) and isinstance(r.get('walls'), list) and 'stats' in r]
if len(reps) != 5:
    fail(f"five dry runs, got {len(reps)} replies")
ms = statistics.median(num((r.get('stats') or {}).get('ms'), 'stats.ms') for r in reps)
walls = [w for w in reps[0]['walls'] if w.get('confidence', 0) >= MIN_CONF]
want = 6 * N * N
print(f"{len(ents)} entities: median {ms:.0f} ms over 5 dry runs; {len(walls)} walls (expected {want})")
if ms >= 2000:
    fail(f"a {len(ents)}-entity plan must be detected in under 2 s (median {ms:.0f} ms)")
if abs(len(walls) - want) > 0.05 * want:
    fail(f"each of the {N * N} tiles has 6 walls: expected about {want}, found {len(walls)}")

# Hostile linework never panics; an accept that fails changes nothing.
cases = []
rnd = lambda: random.choice([random.uniform(-5e3, 5e3), random.uniform(-1e300, 1e300), 0.0, 1e-300, -0.0])
junk = []
for _ in range(1500):
    k = random.random()
    if k < 0.4:
        junk.append(line((rnd(), rnd()), (rnd(), rnd()), random.choice(['A-WALL', '0', 'A-DOOR'])))
    elif k < 0.6:
        junk.append(arc((rnd(), rnd()), random.choice([0, -5, 1e300, random.uniform(1, 2000)]), rnd(), rnd(), 'A-DOOR'))
    elif k < 0.9:
        junk.append(lw([(rnd(), rnd()) for _ in range(random.randint(1, 9))], random.choice(['A-WALL', 'A-GLAZ'])))
    else:
        junk.append(line((100, 100), (100, 100), 'A-WALL'))
cases.append(('random', dxf(junk, insunits=random.choice([0, 1, 2, 5, 6]))))
cases.append(('recursive blocks', dxf([ins('A', (0, 0)), ins('B', (5, 5), 30)], blocks=block('A', [ins('A', (1, 1)), lw([(0, 0), (200, 0), (200, 100)])])
                                      + block('B', [ins('C', (0, 0))]) + block('C', [ins('B', (0, 0), 90)]))))
deep = ''.join(block(f'L{i}', [ins(f'L{i + 1}', (10, 0), 7), lw([(0, 0), (5000, 0), (5000, 200), (0, 200)], 'A-WALL')]) for i in range(60)) + block('L60', [])
cases.append(('60 nested blocks', dxf([ins('L0', (0, 0))], blocks=deep)))
cases.append(('huge MINSERT', dxf([ins('W', (0, 0), cols=100000, rows=100000)], blocks=block('W', [lw([(0, 0), (4000, 0), (4000, 200), (0, 200)], 'A-WALL')]))))
cases.append(('odd units', dxf(tile(0, 0), insunits=99)))
cases.append(('negative units', dxf(tile(0, 0), insunits=-3)))
for name, text in cases:
    p = os.path.join(tmp, name.replace(' ', '_') + '.dxf')
    open(p, 'w').write(text)
    rc, rr, so, se = run(['archdetect {"dryRun":true}', 'archdetect {"accept":"all"}', 'arch.inspect {"walls":true}'], file=p, expect_ok=False, timeout=600)
    if not any(isinstance(r, dict) and isinstance(r.get('walls'), list) and 'revision' in r for r in rr):
        fail(f"{name}: the session must survive hostile linework and still answer arch.inspect: {(so + se)[-400:]}")
    acc = [r for r in rr if isinstance(r, dict) and 'accepted' in r]
    last = [r for r in rr if isinstance(r, dict) and isinstance(r.get('walls'), list) and 'revision' in r][-1]
    if not acc and (last['walls'] or last.get('revision', 0)):
        fail(f"{name}: a failed accept must leave no half-applied edit: {json.dumps(last)[:300]}")
    print(f"ok: {name}: {'accepted ' + str(len(acc[0]['accepted'])) if acc else 'refused cleanly'}")

A = 'examples/apartment.dxf'
for bad in ('{"dryRun":"yes"}', '{"minConfidence":"x"}', '{"minConfidence":7}', '{"minConfidence":-1}', '{"units":"furlong"}',
            '{"layers":"A-WALL"}', '{"layers":[1,2]}', '{"accept":"all","minConfidence":1e400}', '[]', '"all"', '{"accept":null}'):
    rc, rr, so, se = run([f'archdetect {bad}', 'arch.inspect {"walls":true}'], file=A, expect_ok=False)
    last = [r for r in rr if isinstance(r, dict) and isinstance(r.get('walls'), list) and 'revision' in r]
    if not last or last[-1]['walls'] or last[-1].get('revision', 0):
        fail(f"archdetect {bad} must be refused and change nothing: {(so + se)[-300:]}")
print("PASS 08")
