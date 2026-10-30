# `cadcraft-cli convert plan.dxf out.glb` builds 3D from a plain plan: with no arch objects in the drawing it runs
# ARCHDETECT first (accepting "all") and says so on stderr ("archdetect: N walls, ..."); `--no-detect` skips that; a
# drawing that already has arch objects is not detected again.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())


def convert(src, dst, *flags):
    p = subprocess.run(cli_args('convert') + list(flags) + [src, dst], capture_output=True, text=True, timeout=900)
    if 'panicked' in p.stderr + p.stdout:
        fail(f"panic: {(p.stderr + p.stdout)[-800:]}")
    if p.returncode != 0:
        fail(f"convert {' '.join(flags)} {src} exited {p.returncode}: {(p.stderr or p.stdout)[-600:]}")
    return p.stderr


def glb(path):
    b = open(path, 'rb').read()
    if b[:4] != b'glTF':
        fail(f"{path} is not a GLB")
    n = struct.unpack_from('<I', b, 12)[0]
    return json.loads(b[20:20 + n])


def wall_bounds(g):
    lo, hi, n = [1e30] * 3, [-1e30] * 3, 0
    for node in g.get('nodes', []):
        if not str(node.get('name', '')).startswith('wall') or 'mesh' not in node:
            continue
        for prim in g['meshes'][node['mesh']].get('primitives', []):
            a = g['accessors'][prim['attributes']['POSITION']]
            for i in range(3):
                lo[i] = min(lo[i], a['min'][i]); hi[i] = max(hi[i], a['max'][i])
            n += 1
    return n, lo, hi


tmp = tempfile.mkdtemp()
out = os.path.join(tmp, 'apartment.glb')
err = convert('examples/apartment.dxf', out)
if not any(l.startswith('archdetect:') and 'walls' in l for l in err.splitlines()):
    fail(f"convert must say what it detected on stderr ('archdetect: N walls, ...'), got {err[-300:]!r}")
g = glb(out)
names = [str(n.get('name', '')) for n in g.get('nodes', [])]
n, lo, hi = wall_bounds(g)
if not n:
    fail(f"convert built no walls from apartment.dxf: nodes {names[:12]}")
if any(abs(x - y) > 1e-3 for x, y in zip(lo[:2] + hi[:2], [0.0, 0.0, 12.0, 8.0])) or abs(lo[2]) > 1e-6 or hi[2] < 2.0:
    fail(f"the walls must stand on the plan in metres: x 0..12, y 0..8, from z 0 up at least 2 m; got {lo} .. {hi}")
if not any('door' in x for x in names) or not any('window' in x or 'glass' in x for x in names):
    fail(f"the doors and windows must be built too: nodes {names}")
print(f"ok: apartment.glb walls {lo[:2]}..{hi[:2]}, height {hi[2]:.2f}; {len(names)} nodes")

out2 = os.path.join(tmp, 'skip.glb')
convert('examples/apartment.dxf', out2, '--no-detect')
if wall_bounds(glb(out2))[0]:
    fail("convert --no-detect must not build walls from linework")

src = os.path.join(tmp, 'house.dxf')
run(['wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'], save=src)
err3 = convert(src, os.path.join(tmp, 'house.glb'))
if 'archdetect:' in err3:
    fail("a drawing that already has arch objects must not be detected again")
_, lo3, hi3 = wall_bounds(glb(os.path.join(tmp, 'house.glb')))
if abs(hi3[0] - lo3[0] - 10.2) > 1e-3:
    fail(f"the house's own walls are converted as they are (10.2 m wide): {lo3} .. {hi3}")
print("PASS 06")
