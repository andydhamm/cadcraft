# Phase-8 check helpers (ARCHDETECT). A fixture, not a check (the gate runs only *.sh and *.py): each check loads it with
#   exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())
#
# The contract the checks read (metres, the arch model's unit; candidate ids "c0", "c1", ... in one namespace):
#   archdetect {"dryRun":true, "minConfidence"?, "units"?, "layers"?}  (`{}` is a dry run too) replies
#     {"units": "m"|"mm"|..., "scale": metres per drawing unit,
#      "walls":    [{"id","start":[x,y],"end":[x,y],"thickness","confidence","evidence":{"layers":[],"handles":[],"rules":[]}}],
#      "openings": [{"id","kind":"door"|"window"|"cased","host":"<wall candidate id>","center":[x,y],"offset","width","confidence","evidence"}],
#      "rooms":    [{"id","polygon":[[x,y],...],"name":str|null,"area","confidence","evidence"}],
#      "stats":    {"entities","primitives","ms"}}
#   archdetect {"accept":"all"|[ids], "minConfidence"?} replies {"accepted":{"c0":"w0",...},"walls","openings","rooms",
#     "converted": {"from":"mm","scale":0.001} | null}
import json, math, os, random, struct, subprocess, sys, tempfile, zlib

MIN_CONF = 0.5
HERE = os.path.dirname(os.path.abspath(__file__))


def fail(msg):
    print("FAIL: " + msg)
    sys.exit(1)


def cli_args(sub, release=False):
    exe = os.environ.get('CADCRAFT_CLI')            # a prebuilt binary, for trying the checks by hand
    if exe:
        return [exe, sub]
    return ['cargo', 'run', '-q'] + (['--release'] if release else []) + ['-p', 'cadcraft-cli', '--', sub]


def parse_rows(out):
    rows = []
    for line in out.splitlines():
        line = line.strip()
        if line.startswith('{') or line.startswith('['):
            try:
                rows.append(json.loads(line))
            except ValueError:
                pass
    return rows


def run(cmds, file=None, save=None, expect_ok=True, release=False, timeout=900):
    """cadcraft-cli run: one JSON reply per line. Returns (rc, replies, stdout, stderr). A panic always fails."""
    args = cli_args('run', release) + ([file] if file else ['--metric'])
    for c in cmds:
        args += ['--cmd', c]
    if save:
        args += ['--save', save]
    p = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    if 'panicked' in p.stderr or 'panicked' in p.stdout:
        fail(f"panic: {(p.stderr + p.stdout)[-800:]}")
    if expect_ok and p.returncode != 0:
        fail(f"cadcraft-cli run exited {p.returncode}: {(p.stderr or p.stdout)[-800:]}")
    return p.returncode, parse_rows(p.stdout), p.stdout, p.stderr


def reply_with(rows, key, nth=-1, what=None):
    hits = [r for r in rows if isinstance(r, dict) and key in r]
    if not hits or not (-len(hits) <= nth < len(hits)):
        fail(f"no reply with '{key}' ({what or key}); replies: {json.dumps(rows)[:600]}")
    return hits[nth]


def reply_list(rows, key, what=None):
    """The last reply whose `key` is a list (other replies may carry the same key as a count)."""
    hits = [r for r in rows if isinstance(r, dict) and isinstance(r.get(key), list)]
    if not hits:
        fail(f"no reply with a '{key}' list ({what or key}); replies: {json.dumps(rows)[:600]}")
    return hits[-1]


def num(x, what):
    if not isinstance(x, (int, float)) or isinstance(x, bool) or not math.isfinite(x):
        fail(f"{what} is not a finite number: {x!r}")
    return float(x)


def pt(p, what):
    if not isinstance(p, list) or len(p) != 2:
        fail(f"{what} is not [x,y]: {p!r}")
    return [num(p[0], what), num(p[1], what)]


# ---- DXF text, as group-code / value pairs ----

def pairs_of(text):
    lines = text.replace('\r\n', '\n').split('\n')
    return [[lines[i].strip(), lines[i + 1]] for i in range(0, len(lines) - 1, 2)]


def text_of(pairs):
    return ''.join(f'{c}\n{v}\n' for c, v in pairs)


def sections(pairs):
    """[(section name, start index, end index)] over the pair list."""
    out, cur, start = [], None, 0
    for i, (c, v) in enumerate(pairs):
        if c == '2' and i and pairs[i - 1] == ['0', 'SECTION']:
            cur, start = v.strip(), i
        elif c == '0' and v.strip() == 'ENDSEC' and cur:
            out.append((cur, start, i)); cur = None
    return out


def header_value(text, name):
    p = pairs_of(text)
    for i, (c, v) in enumerate(p):
        if c == '9' and v.strip() == name and i + 1 < len(p):
            return p[i + 1][1].strip()
    return None


def set_header_int(text, name, value):
    p = pairs_of(text)
    for i, (c, v) in enumerate(p):
        if c == '9' and v.strip() == name and i + 1 < len(p):
            p[i + 1][1] = str(value)
            return text_of(p)
    fail(f"no {name} in the DXF header")


def strip_arch(text):
    """Plain linework, as another program would leave it: the CADCRAFT_ARCH dictionary entry and its XRECORD are removed,
    and every CADCRAFT xdata run on an entity."""
    p = pairs_of(text)
    rec = None
    for i, (c, v) in enumerate(p):
        if c == '3' and v.strip() == 'CADCRAFT_ARCH' and i + 1 < len(p) and p[i + 1][0] == '350':
            rec = p[i + 1][1].strip()
    out, i, skip = [], 0, False
    while i < len(p):
        c, v = p[i]
        if c == '3' and v.strip() == 'CADCRAFT_ARCH':
            i += 2; continue
        if c == '0' and rec and i + 1 < len(p) and p[i + 1][0] == '5' and p[i + 1][1].strip() == rec:
            i += 1
            while i < len(p) and p[i][0] != '0':
                i += 1
            continue
        if c == '1001':
            skip = v.strip() == 'CADCRAFT'
            if skip:
                i += 1; continue
        elif c == '0':
            skip = False
        if skip and c.isdigit() and int(c) >= 1000:
            i += 1; continue
        out.append([c, v]); i += 1
    t = text_of(out)
    for bad in ('CADCRAFT_ARCH', 'ARCHWALL', 'ARCHOPENING', '"version"'):
        if bad in t:
            fail(f"could not strip the arch record ({bad} is still in the file)")
    return t


def entity_handles(text):
    p = pairs_of(text)
    for name, s, e in sections(p):
        if name == 'ENTITIES':
            hs, i = [], s
            while i < e:
                if p[i][0] == '0' and i + 1 < e and p[i + 1][0] == '5':
                    hs.append(p[i + 1][1].strip())
                i += 1
            return hs
    return []


def edit_insert(text, block, at=None, rotation=None, scale=None):
    """Move, turn (degrees) or scale the model-space INSERT of `block`."""
    p = pairs_of(text)
    for name, s, e in sections(p):
        if name != 'ENTITIES':
            continue
        i = s
        while i < e:
            if p[i] == ['0', 'INSERT']:
                j = i + 1
                body = []
                while j < e and p[j][0] != '0':
                    body.append(j); j += 1
                if any(p[k][0] == '2' and p[k][1].strip() == block for k in body):
                    for k in body:
                        c = p[k][0]
                        if at and c in ('10', '20'):
                            p[k][1] = repr(float(at[0] if c == '10' else at[1]))
                        if rotation is not None and c == '50':
                            p[k][1] = repr(float(rotation))
                        if scale is not None and c in ('41', '42', '43'):
                            p[k][1] = repr(float(scale))
                    return text_of(p)
                i = j
            else:
                i += 1
    fail(f"no INSERT of {block} in model space")


def relayer(text, layer):
    """Every entity (model space and block bodies) on one layer: what a plan without layer names looks like."""
    p = pairs_of(text)
    for name, s, e in sections(p):
        if name in ('ENTITIES', 'BLOCKS'):
            ent = False
            for k in range(s, e):
                c, v = p[k]
                if c == '0':
                    ent = v.strip() not in ('BLOCK', 'ENDBLK', 'SECTION', 'ENDSEC')
                elif c == '8' and ent:
                    p[k][1] = layer
    return text_of(p)


# ---- truth ----

def unit(a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    n = math.hypot(dx, dy)
    return (dx / n, dy / n) if n > 0 else (0.0, 0.0)


def truth_from_inspect(insp):
    """Walls as centre lines with thickness, openings as kind, centre and width (centre = wall start + offset along it)."""
    walls, by_id = [], {}
    for w in insp.get('walls') or []:
        t = {'id': w['id'], 'a': pt(w['start'], 'wall start'), 'b': pt(w['end'], 'wall end'), 't': num(w['thickness'], 'thickness')}
        walls.append(t); by_id[w['id']] = t
    ops = []
    for o in insp.get('openings') or []:
        if o.get('kind') not in ('door', 'window'):
            continue
        w = by_id.get(o['host'])
        if not w:
            fail(f"inspect opening {o.get('id')} has no host wall")
        d = unit(w['a'], w['b'])
        off = num(o['offset'], 'offset')
        ops.append({'id': o['id'], 'kind': o['kind'], 'c': [w['a'][0] + d[0] * off, w['a'][1] + d[1] * off], 'w': num(o['width'], 'width')})
    return {'walls': walls, 'openings': ops}


def transform(truth, rot_deg=0.0, at=(0.0, 0.0), scale=1.0):
    """The truth after an INSERT with that rotation (degrees, about the origin), uniform scale and insertion point."""
    c, s = math.cos(math.radians(rot_deg)), math.sin(math.radians(rot_deg))
    f = lambda q: [at[0] + scale * (c * q[0] - s * q[1]), at[1] + scale * (s * q[0] + c * q[1])]
    return {'walls': [dict(w, a=f(w['a']), b=f(w['b']), t=w['t'] * scale) for w in truth['walls']],
            'openings': [dict(o, c=f(o['c']), w=o['w'] * scale) for o in truth['openings']]}


# ---- detection ----

def dry_reply(rows, nth=-1):
    """An archdetect dry-run reply: the one carrying candidate lists and stats."""
    hits = [r for r in rows if isinstance(r, dict) and isinstance(r.get('walls'), list) and 'stats' in r]
    if not hits or not (-len(hits) <= nth < len(hits)):
        fail(f"no archdetect dry-run reply (walls, openings, rooms and stats); replies: {json.dumps(rows)[:400]}")
    return hits[nth]


def detect(path, extra=None, release=False):
    params = dict({'dryRun': True}, **(extra or {}))
    _, rows, _, _ = run([f'archdetect {json.dumps(params)}'], file=path, release=release)
    return dry_reply(rows)


def candidates(r, minc=MIN_CONF):
    """The candidates an accept of "all" would create, checked for shape. Coordinates in metres."""
    for k in ('walls', 'openings', 'rooms'):
        if not isinstance(r.get(k), list):
            fail(f"archdetect reply has no '{k}' list: {json.dumps(r)[:300]}")
    ids = set()
    walls, ops = [], []
    for w in r['walls']:
        if not isinstance(w.get('id'), str) or w['id'] in ids:
            fail(f"wall candidate needs a unique string id: {w!r:.200}")
        ids.add(w['id'])
        c = num(w.get('confidence'), f"{w['id']} confidence")
        if not 0.0 <= c <= 1.0:
            fail(f"{w['id']} confidence {c} is not in 0..1")
        ev = w.get('evidence')
        if not isinstance(ev, dict) or not isinstance(ev.get('rules'), list) or not ev['rules']:
            fail(f"{w['id']} must say which evidence it used (evidence.rules): {w!r:.300}")
        if c >= minc:
            walls.append({'id': w['id'], 'a': pt(w.get('start'), 'start'), 'b': pt(w.get('end'), 'end'), 't': num(w.get('thickness'), 'thickness')})
    for o in r['openings']:
        if not isinstance(o.get('id'), str) or o['id'] in ids:
            fail(f"opening candidate needs a unique string id: {o!r:.200}")
        ids.add(o['id'])
        c = num(o.get('confidence'), f"{o['id']} confidence")
        ev = o.get('evidence')
        if not isinstance(ev, dict) or not isinstance(ev.get('rules'), list) or not ev['rules']:
            fail(f"{o['id']} must say which evidence it used (evidence.rules): {o!r:.300}")
        if c >= minc:
            ops.append({'id': o['id'], 'kind': o.get('kind'), 'host': o.get('host'), 'c': pt(o.get('center'), 'center'), 'w': num(o.get('width'), 'width')})
    return walls, ops


def seg_dist(p, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    L2 = dx * dx + dy * dy
    t = 0.0 if L2 == 0 else max(0.0, min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / L2))
    return math.hypot(p[0] - a[0] - t * dx, p[1] - a[1] - t * dy)


def wall_fit(c, t):
    """None when candidate c does not match truth wall t, else its distance: the centre lines within half the truth's
    thickness (at least 50 mm) of each other over their whole length (both ways), thickness within 20 %."""
    tol = max(t['t'] / 2.0, 0.05)
    h = max(seg_dist(c['a'], t['a'], t['b']), seg_dist(c['b'], t['a'], t['b']), seg_dist(t['a'], c['a'], c['b']), seg_dist(t['b'], c['a'], c['b']))
    if h > tol + 1e-9 or abs(c['t'] - t['t']) > 0.2 * t['t'] + 1e-9:
        return None
    return h


def opening_fit(c, t):
    """Kind agrees, centre within 0.15 m, width within 10 %."""
    if c['kind'] != t['kind']:
        return None
    d = math.hypot(c['c'][0] - t['c'][0], c['c'][1] - t['c'][1])
    if d > 0.15 + 1e-9 or abs(c['w'] - t['w']) > 0.1 * t['w'] + 1e-9:
        return None
    return d


def match(cands, truths, fit):
    """One-to-one: the closest pairs first. Returns (matched pairs, unmatched candidates, unmatched truths)."""
    pairs = sorted((f, i, j) for i, c in enumerate(cands) for j, t in enumerate(truths) for f in [fit(c, t)] if f is not None)
    used_c, used_t, out = set(), set(), []
    for f, i, j in pairs:
        if i not in used_c and j not in used_t:
            used_c.add(i); used_t.add(j); out.append((cands[i], truths[j]))
    return out, [c for i, c in enumerate(cands) if i not in used_c], [t for j, t in enumerate(truths) if j not in used_t]


class Tally:
    def __init__(self):
        self.n = {k: [0, 0, 0] for k in ('walls', 'doors', 'windows')}       # matched, candidates, truths
        self.notes = []

    def add(self, name, r, truth, minc=MIN_CONF):
        walls, ops = candidates(r, minc)
        m, fp, fn = match(walls, truth['walls'], wall_fit)
        self._count('walls', len(m), len(walls), len(truth['walls']))
        self._note(name, 'wall', fp, fn, lambda w: f"{w['id']} {fmt(w['a'])}-{fmt(w['b'])} t{w['t']:.3f}")
        for kind, key in (('door', 'doors'), ('window', 'windows')):
            cs = [o for o in ops if o['kind'] == kind]
            ts = [o for o in truth['openings'] if o['kind'] == kind]
            m, fp, fn = match(cs, ts, opening_fit)
            self._count(key, len(m), len(cs), len(ts))
            self._note(name, kind, fp, fn, lambda o: f"{o['id']} at {fmt(o['c'])} w{o['w']:.3f}")

    def _count(self, k, m, c, t):
        self.n[k][0] += m; self.n[k][1] += c; self.n[k][2] += t

    def _note(self, name, what, fp, fn, show):
        for x in fp[:6]:
            self.notes.append(f"  {name}: extra {what} {show(x)}")
        for x in fn[:6]:
            self.notes.append(f"  {name}: missed {what} {show(x)}")

    def pr(self, k):
        m, c, t = self.n[k]
        return (m / c if c else (1.0 if not t else 0.0)), (m / t if t else 1.0)

    def report(self):
        return '; '.join(f"{k} P {self.pr(k)[0]:.3f} R {self.pr(k)[1]:.3f} ({self.n[k][0]} of {self.n[k][1]} found, {self.n[k][2]} true)" for k in self.n)


def fmt(p):
    return f"({p[0]:.3f},{p[1]:.3f})"


def inside(p, poly):
    x, y, c = p[0], p[1], False
    for i in range(len(poly)):
        (x1, y1), (x2, y2) = poly[i], poly[(i + 1) % len(poly)]
        if (y1 > y) != (y2 > y) and x < (x2 - x1) * (y - y1) / (y2 - y1) + x1:
            c = not c
    return c


# ---- the synthetic corpus: plans built from our own models ----

def loop(points, t, exterior=True):
    return f'wall {json.dumps({"points": points, "thickness": t, "height": 2.4, "exterior": exterior})}'


def door(w, off, width, **k):
    return 'door ' + json.dumps(dict({'wall': w, 'offset': off, 'width': width, 'height': 2.1}, **k))


def window(w, off, width, **k):
    return 'window ' + json.dumps(dict({'wall': w, 'offset': off, 'width': width, 'height': 1.2, 'sill': 0.9}, **k))


def rot(points, deg):
    c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    return [[round(c * x - s * y, 9), round(s * x + c * y, 9)] for x, y in points]


PLANS = {
    # 10 x 8 house, two partitions in a T, doors both ways, four window styles.
    'house': [loop([[0, 0], [10, 0], [10, 8], [0, 8], [0, 0]], 0.2), loop([[6, 0], [6, 8]], 0.1, False),
              loop([[6, 4.5], [10, 4.5]], 0.1, False),
              door('w0', 3.0, 0.9, style='hinged'), window('w0', 8.0, 1.2, style='slider'), door('w4', 2.0, 0.8),
              door('w5', 2.0, 0.8, hinge='right', swing='out'), window('w1', 6.2, 1.0, style='casement'),
              window('w2', 2.0, 1.8, style='picture'), window('w3', 4.0, 1.2)],
    # L-shaped, 300 mm exterior and 120 mm partitions, a sliding door.
    'ell': [loop([[0, 0], [12, 0], [12, 5], [7, 5], [7, 9], [0, 9], [0, 0]], 0.3), loop([[4, 0], [4, 9]], 0.12, False),
            loop([[0, 4.5], [4, 4.5]], 0.12, False),
            door('w0', 9.5, 0.9), window('w0', 2.0, 1.5), window('w1', 2.5, 1.2), door('w6', 7.0, 0.8),
            door('w7', 2.0, 0.75), window('w5', 2.0, 1.0), window('w4', 4.5, 2.0), door('w2', 2.5, 1.8, style='sliding')],
    # A 9 x 7 plan turned 30 degrees, 250 mm walls.
    'turned': [loop(rot([[0, 0], [9, 0], [9, 7], [0, 7], [0, 0]], 30), 0.25), loop(rot([[5, 0], [5, 7]], 30), 0.1, False),
               door('w0', 2.5, 0.9), window('w1', 3.5, 1.4), window('w2', 2.0, 2.0), door('w4', 2.0, 0.8, hinge='right')],
    # A long cabin: two partitions, windows close together.
    'cabin': [loop([[0, 0], [14, 0], [14, 6], [0, 6], [0, 0]], 0.2), loop([[5, 0], [5, 6]], 0.1, False), loop([[9, 0], [9, 6]], 0.1, False),
              window('w0', 1.5, 1.0), window('w0', 3.3, 1.0), door('w0', 7.0, 0.9), window('w0', 11.5, 1.5),
              window('w2', 2.0, 1.2), window('w2', 7.0, 1.2), door('w4', 3.0, 0.8), door('w5', 3.0, 0.8, hinge='right'),
              window('w1', 3.0, 0.9), window('w3', 3.0, 0.9)],
    # A 45-degree corner.
    'bay': [loop([[0, 0], [10, 0], [10, 5], [8, 7], [0, 7], [0, 0]], 0.2), loop([[5, 0], [5, 7]], 0.1, False),
            window('w2', 1.414, 1.0), door('w0', 2.5, 0.9), window('w0', 7.5, 1.5), door('w5', 5.0, 0.8), window('w3', 4.0, 1.2)],
}


def build(name, tmp):
    """Build plan `name` with the commands, save it, and return (its saved DXF text, its truth from arch.inspect)."""
    path = os.path.join(tmp, name + '.dxf')
    _, rows, _, _ = run(PLANS[name] + ['arch.inspect {"walls":true,"openings":true}'], save=path)
    insp = reply_with(rows, 'walls', what='arch.inspect')
    truth = truth_from_inspect(insp)
    want_o = sum(1 for c in PLANS[name] if c.startswith(('door', 'window')))
    if len(truth['openings']) != want_o:
        fail(f"{name}: {want_o} openings were placed but inspect lists {len(truth['openings'])}")
    return open(path).read(), truth


def plain(name, text, tmp):
    """The plan as plain linework in metres (the arch record and xdata stripped, $INSUNITS 6)."""
    path = os.path.join(tmp, name + '.plain.dxf')
    t = strip_arch(text)
    t = set_header_int(t, '$INSUNITS', 6)
    open(path, 'w').write(t)
    return path


def wrapped(name, plain_path, tmp, at, rotation, scale=1.0, insunits=None):
    """The plain plan inside two nested blocks (PLAN inside SITE); the SITE insert moved, turned and scaled."""
    _, rows, _, _ = run([f'block {json.dumps({"name": "PLAN", "base": [0, 0], "handles": entity_handles(open(plain_path).read()), "keep": "convert"})}'],
                        file=plain_path, save=os.path.join(tmp, name + '.b1.dxf'))
    ins = reply_with(rows, 'insert', what='block PLAN')['insert']
    out = os.path.join(tmp, name + '.b2.dxf')
    run([f'block {json.dumps({"name": "SITE", "base": [0, 0], "handles": [ins], "keep": "convert"})}'], file=os.path.join(tmp, name + '.b1.dxf'), save=out)
    t = edit_insert(open(out).read(), 'SITE', at=at, rotation=rotation, scale=scale)
    if insunits is not None:
        t = set_header_int(t, '$INSUNITS', insunits)
    open(out, 'w').write(t)
    return out


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


def keep_artifact(path, name=None):
    """Leave a copy of a picture for the gate (CHECK_ARTIFACTS_DIR, when the gate gives one)."""
    d = os.environ.get('CHECK_ARTIFACTS_DIR')
    if d and os.path.isfile(path):
        import shutil
        shutil.copy(path, os.path.join(d, name or os.path.basename(path)))
