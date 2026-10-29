# Phase-7 check helpers. A fixture, not a check (the gate runs only *.sh and *.py): each check loads it with
#   exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
import json, math, os, struct, subprocess, sys, tempfile, zlib

# The phase-1 house with the phase-2 front door at offset 3.0.
HOUSE = [
    'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}',
    'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}',
]
# Rooms and the 6/12 gable roof with a 0.5 m overhang, gable ends on the south (front) and north walls.
ROOF = [
    'roomdetect {}',
    'roofauto {"pitchRise":6,"pitchRun":12,"overhang":0.5,"walls":[{"wall":"w0","directive":"gable"},{"wall":"w2","directive":"gable"}]}',
]
TOL = 1e-4


def fail(msg):
    print("FAIL: " + msg)
    sys.exit(1)


def run(cmds, file=None, save=None, expect_ok=True):
    """cadcraft-cli run: one JSON reply per line (null replies print nothing). Returns (rc, replies, stdout, stderr)."""
    args = ['cargo', 'run', '-q', '-p', 'cadcraft-cli', '--', 'run']
    args += [file] if file else ['--metric']
    for c in cmds:
        args += ['--cmd', c]
    if save:
        args += ['--save', save]
    p = subprocess.run(args, capture_output=True, text=True, timeout=900)
    if 'panicked' in p.stderr or 'panicked' in p.stdout:
        fail(f"panic: {(p.stderr + p.stdout)[-800:]}")
    rows = []
    for line in p.stdout.splitlines():
        line = line.strip()
        if line.startswith('{') or line.startswith('['):
            try:
                rows.append(json.loads(line))
            except ValueError:
                pass
    if expect_ok and p.returncode != 0:
        fail(f"cadcraft-cli run exited {p.returncode}: {(p.stderr or p.stdout)[-800:]}")
    return p.returncode, rows, p.stdout, p.stderr


def reply_with(rows, key, nth=-1, what=None):
    """The nth reply (default: the last) that is an object carrying `key`."""
    hits = [r for r in rows if isinstance(r, dict) and key in r]
    if not hits or not (-len(hits) <= nth < len(hits)):
        fail(f"no reply with '{key}' ({what or key}); replies: {json.dumps(rows)[:600]}")
    return hits[nth]


def views_of(rows, nth=-1):
    v = reply_with(rows, 'views', nth, 'arch.inspect {"views":true}')['views']
    if not isinstance(v, list):
        fail(f"arch.inspect views is not a list: {v!r:.300}")
    return v


def num(x, what):
    if not isinstance(x, (int, float)) or isinstance(x, bool) or not math.isfinite(x):
        fail(f"{what} is not a finite number: {x!r}")
    return float(x)


def pt(p, what):
    if not isinstance(p, list) or len(p) != 2:
        fail(f"{what} is not [u,v]: {p!r}")
    return [num(p[0], what), num(p[1], what)]


def lines_of(view):
    out = []
    for i, l in enumerate(view.get('lines') or []):
        if not isinstance(l, dict) or not isinstance(l.get('kind'), str) or not isinstance(l.get('id'), str):
            fail(f"view {view.get('id')} line {i} needs a, b, kind, id: {l!r:.200}")
        out.append({'a': pt(l.get('a'), f'line {i} a'), 'b': pt(l.get('b'), f'line {i} b'), 'kind': l['kind'], 'id': l['id']})
    return out


def extent(lines, rings=()):
    us = [p[0] for l in lines for p in (l['a'], l['b'])] + [p[0] for r in rings for p in r['points']]
    vs = [p[1] for l in lines for p in (l['a'], l['b'])] + [p[1] for r in rings for p in r['points']]
    if not us:
        fail("the view has no geometry")
    return [min(us), min(vs), max(us), max(vs)]


def covers(lines, a, b, tol=TOL):
    """Whether collinear lines cover the segment a-b (their union, within tol)."""
    ln = math.hypot(b[0] - a[0], b[1] - a[1])
    d = [(b[0] - a[0]) / ln, (b[1] - a[1]) / ln]
    spans = []
    for l in lines:
        off = lambda p: (p[0] - a[0]) * -d[1] + (p[1] - a[1]) * d[0]
        if abs(off(l['a'])) > tol or abs(off(l['b'])) > tol:
            continue
        s = lambda p: (p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]
        spans.append(tuple(sorted((s(l['a']), s(l['b'])))))
    reach = 0.0
    for lo, hi in sorted(spans):
        if lo > reach + tol:
            break
        reach = max(reach, hi)
    return reach >= ln - tol


def closed_rect(lines, w, h, tol=TOL):
    """Left u of a closed axis-aligned w x h rectangle drawn by the lines (two jambs, head and sill), or None."""
    jambs = [l for l in lines if abs(l['a'][0] - l['b'][0]) < tol and abs(abs(l['a'][1] - l['b'][1]) - h) < tol]
    for j in jambs:
        x, y0 = j['a'][0], min(j['a'][1], j['b'][1])
        for k in jambs:
            if abs(k['a'][0] - x - w) < tol and abs(min(k['a'][1], k['b'][1]) - y0) < tol \
               and covers(lines, [x, y0 + h], [x + w, y0 + h], tol) and covers(lines, [x, y0], [x + w, y0], tol):
                return x
    return None


def shoelace(points):
    return 0.5 * sum(points[i][0] * points[(i + 1) % len(points)][1] - points[(i + 1) % len(points)][0] * points[i][1]
                     for i in range(len(points)))


def png(path):
    b = open(path, 'rb').read()
    if b[:8] != b'\x89PNG\r\n\x1a\n':
        fail(f"{path} is not a PNG")
    i = 8; w = h = 0; idat = b''; ct = 6; depth = 8
    while i < len(b):
        ln, typ = struct.unpack_from('>I4s', b, i); body = b[i + 8:i + 8 + ln]; i += 12 + ln
        if typ == b'IHDR':
            w, h, depth, ct = struct.unpack('>IIBB', body[:10])
        elif typ == b'IDAT':
            idat += body
        elif typ == b'IEND':
            break
    if depth != 8 or ct not in (0, 2, 4, 6):
        fail(f"unsupported PNG (depth {depth}, colour type {ct})")
    nch = {0: 1, 2: 3, 4: 2, 6: 4}[ct]; stride = w * nch; raw = zlib.decompress(idat); q = 0; prev = bytearray(stride); rows = []
    for _ in range(h):
        f = raw[q]; q += 1; line = bytearray(raw[q:q + stride]); q += stride
        if f == 1:
            for j in range(nch, stride): line[j] = (line[j] + line[j - nch]) & 255
        elif f == 2:
            for j in range(stride): line[j] = (line[j] + prev[j]) & 255
        elif f == 3:
            for j in range(stride): line[j] = (line[j] + ((line[j - nch] if j >= nch else 0) + prev[j]) // 2) & 255
        elif f == 4:
            for j in range(stride):
                a = line[j - nch] if j >= nch else 0; bb = prev[j]; c = prev[j - nch] if j >= nch else 0
                pp = a + bb - c; pa = abs(pp - a); pb = abs(pp - bb); pc = abs(pp - c)
                line[j] = (line[j] + (a if pa <= pb and pa <= pc else bb if pb <= pc else c)) & 255
        rows.append(bytes(line)); prev = line
    return w, h, nch, rows
