# Views live in the arch model: they round-trip through DXF (CADCRAFT_ARCH v7) and come back regenerated; their
# linework is written as proxies on A-ELEV / A-SECT only on save, and dropped on read (never duplicated).
import os, re
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
tmp = tempfile.mkdtemp()
a, b2 = os.path.join(tmp, 'a.dxf'), os.path.join(tmp, 'b.dxf')
_, rows, _, _ = run(HOUSE + ROOF + ['elevation {"side":"south","insert":[0,-20],"name":"Front"}',
                                    'section {"from":[-1,4],"to":[11,4],"look":[0,1],"insert":[0,-40],"scale":100}',
                                    'arch.inspect {"views":true}'], save=a)
before = views_of(rows)
_, rows2, _, _ = run(['arch.inspect {"views":true}'], file=a, save=b2)
after = views_of(rows2)
def summary(v):
    return (v.get('id'), v.get('kind'), v.get('name'), v.get('layer'), json.dumps(v.get('look')), json.dumps(v.get('cut')),
            json.dumps(v.get('insert')), v.get('scale'), len(v.get('lines') or []), len(v.get('rings') or []),
            [round(x, 6) for x in v.get('extent') or []])
if [summary(v) for v in before] != [summary(v) for v in after]:
    fail(f"views changed across save/open:\n{[summary(v) for v in before]}\n{[summary(v) for v in after]}")
if after[0].get('name') != 'Front' or not isinstance(after[1].get('name'), str) or not after[1]['name']:
    fail(f"view names: {[v.get('name') for v in after]} (want 'Front' and a default name)")
if after[1].get('scale') != 100:
    fail(f"section scale {after[1].get('scale')}, want 100")
def proxies(path):
    t = open(path, 'r', errors='replace').read().replace('\r\n', '\n')
    # A section's name follows group code 2: '  0 SECTION / 2 ENTITIES' (10-29: splitting on '0 ENTITIES' matched
    # nothing and counted the HEADER, so a file full of proxies read as none).
    m = re.search(r'\n  0\nSECTION\n  2\nENTITIES\n(.*?)\n  0\nENDSEC', '\n' + t, re.S)
    if not m:
        fail(f"{os.path.basename(path)} has no ENTITIES section")
    ents = m.group(1)
    return {lay: len(re.findall(r'\n  8\n' + lay + r'\n', '\n' + ents)) for lay in ('A-ELEV', 'A-SECT')}
pa, pb = proxies(a), proxies(b2)
if pa['A-ELEV'] == 0 or pa['A-SECT'] == 0:
    fail(f"no view proxies written on save: {pa}")
if pa != pb:
    fail(f"proxies duplicated or lost on open + save: {pa} -> {pb}")
print(f"ok: 2 views round-trip; proxies {pa}")
