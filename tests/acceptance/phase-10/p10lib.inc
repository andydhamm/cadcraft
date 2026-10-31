# Phase-10 check helpers (lighting, light fixtures, Finalize Render). A fixture, not a check (the gate runs only *.sh and *.py):
#   exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())
#
# The commands the checks drive (all lengths in metres, angles in degrees):
#   lightfixture {type?, at?: [x,y], room?: "r0", rows?, cols?, level?, height?, rotation?, tilt?, lumens?, kelvin?, cone?, falloff?, kind?, on?}
#     -> {"lights": ["lf0", ...], "handles": ["a0048000", ...]}
#   lightfixture.edit {id, at?, ...} -> {"light": "lf0"};  lightfixture.delete {id};  lightfixture.types {} -> {"types": [...]}
#   arch.inspect {"lights": true} -> {"lights": [{id, handle, fixture, shape, kind, mount, level, at, z, rotation, tilt, lumens,
#                                                  kelvin, cone, falloff, candela, on}]}
#   illuminance {point, normal?} -> {"lux", "lights", "direct": true, "contributions": [{id, lux}]}
#   build3d {probe?} -> {..., "lights", "fixtures", "shadowMapsBuilt", "finalRenders", "probe": [{id, part, level} | null]}
#   render {out?, size?, camera?, final?, sun?: false | {date, time} | "default", style?, ground?, budget?, samples?}
#     -> {"ok", "out", "size", "final", "lights", "drawnLights", "shadowedLights", "shadowMapsBuilt", "finalRenders",
#         "frameShadowMaps", "samples": [{point, pixel, rgb, visible}], "device"}
#   sunstudy {date, from, to, step, out, size?, camera?, final?, ground?} -> {"date", "stills": [{time, azimuth, elevation, file, belowHorizon}], "count"}
#   cadcraft-cli render FILE OUT.png [--night] [--final] [--budget N] [--camera …] [--size WxH] [--adapter fallback]
#     stderr: LIGHTS n shadowed=k drawn=m / SHADOWMAPS built=N / FINAL renders=1 size=WxH
# `run` sessions get the software adapter (CADCRAFT_RENDER_ADAPTER=fallback) unless the environment sets another.
import codecs, json, math, os, re, struct, subprocess, sys, tempfile, zlib

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
    env = dict(os.environ)
    env.setdefault('CADCRAFT_RENDER_ADAPTER', 'fallback')
    p = subprocess.run(args, capture_output=True, text=True, timeout=timeout, env=env)
    if 'panicked' in p.stderr or 'panicked' in p.stdout:
        fail(f"panic: {(p.stderr + p.stdout)[-800:]}")
    if expect_ok and p.returncode != 0:
        fail(f"cadcraft-cli run exited {p.returncode}: {(p.stderr or p.stdout)[-800:]}")
    return p.returncode, parse_rows(p.stdout), p.stdout, p.stderr


def was_refused(rc, se):
    """A refused step: `cadcraft-cli run` reports a failing --cmd and goes on, and only the last step's failure sets the
    exit status, so a refusal followed by arch.inspect shows as an `error:` line on stderr with rc 0 (operator 10-30)."""
    return rc != 0 or any(l.startswith('error: ') for l in se.splitlines())


def cli(sub, args, expect_ok=True, timeout=900):
    p = subprocess.run(cli_args(sub) + args, capture_output=True, text=True, timeout=timeout)
    if 'panicked' in p.stderr or 'panicked' in p.stdout:
        fail(f"panic: {(p.stderr + p.stdout)[-800:]}")
    if expect_ok and p.returncode != 0:
        fail(f"cadcraft-cli {sub} exited {p.returncode}: {(p.stderr or p.stdout)[-800:]}")
    return p


def cmd(name, params):
    return f'{name} {json.dumps(params)}'


def reply_with(rows, key, nth=-1, what=None):
    hits = [r for r in rows if isinstance(r, dict) and key in r]
    if not hits or not (-len(hits) <= nth < len(hits)):
        fail(f"no reply with '{key}' ({what or key}); replies: {json.dumps(rows)[:600]}")
    return hits[nth]


def num(x, what):
    if not isinstance(x, (int, float)) or isinstance(x, bool) or not math.isfinite(x):
        fail(f"{what} is not a finite number: {x!r}")
    return float(x)


def near(got, want, tol, what):
    g = num(got, what)
    if abs(g - want) > tol:
        fail(f"{what}: expected {want} ± {tol}, got {g}")
    return g


def probes(rows):
    r = [x for x in rows if isinstance(x, dict) and isinstance(x.get('probe'), list)]
    if not r:
        fail(f"no build3d reply with a probe list: {json.dumps(rows)[:500]}")
    return r[-1]


def part(hit):
    return hit.get('part') if isinstance(hit, dict) else None


# ---- GLB ----

def glb(path):
    b = open(path, 'rb').read()
    if b[:4] != b'glTF':
        fail(f"{path} is not a GLB")
    n = struct.unpack_from('<I', b, 12)[0]
    j = json.loads(b[20:20 + n])
    rest = 20 + n
    binc = b''
    if rest + 8 <= len(b):
        ln = struct.unpack_from('<I', b, rest)[0]
        binc = b[rest + 8:rest + 8 + ln]
    return j, binc


def _acc(j, binc, i):
    a = j['accessors'][i]; v = j['bufferViews'][a['bufferView']]
    off = v.get('byteOffset', 0) + a.get('byteOffset', 0)
    comp = {5126: ('f', 4), 5125: ('I', 4), 5123: ('H', 2), 5121: ('B', 1)}[a['componentType']]
    width = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3}[a['type']]
    stride = v.get('byteStride', comp[1] * width)
    return [struct.unpack_from('<' + comp[0] * width, binc, off + k * stride) for k in range(a['count'])]


def node_meshes(j, name):
    out = []
    for nd in j.get('nodes', []):
        if nd.get('name') == name and 'mesh' in nd:
            out.append(j['meshes'][nd['mesh']])
    return out


def node_box(j, name):
    """GLTF is Y-up: the box comes back in drawing axes (x, y = -gltf z, z = gltf y) when the writer rotates, so take
    min/max over the position accessors and let the caller compare spans."""
    lo, hi = [math.inf] * 3, [-math.inf] * 3
    ms = node_meshes(j, name)
    if not ms:
        fail(f"GLB has no node '{name}'; nodes: {[n.get('name') for n in j.get('nodes', [])][:40]}")
    for m in ms:
        for prim in m['primitives']:
            a = j['accessors'][prim['attributes']['POSITION']]
            for k in range(3):
                lo[k] = min(lo[k], a['min'][k]); hi[k] = max(hi[k], a['max'][k])
    return lo, hi


def closed(j, binc, name):
    """Every edge of the node's triangles (positions welded at 1e-5) is used exactly twice."""
    for m in node_meshes(j, name):
        for prim in m['primitives']:
            pos = _acc(j, binc, prim['attributes']['POSITION'])
            idx = [t[0] for t in _acc(j, binc, prim['indices'])] if 'indices' in prim else list(range(len(pos)))
            key = [tuple(round(c / 1e-5) for c in p) for p in pos]
            edges = {}
            for t in range(0, len(idx) - 2, 3):
                tri = [key[idx[t]], key[idx[t + 1]], key[idx[t + 2]]]
                if len(set(tri)) < 3:
                    continue
                for a, b in ((tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])):
                    e = (a, b) if a < b else (b, a)
                    edges[e] = edges.get(e, 0) + 1
            bad = [c for c in edges.values() if c != 2]
            if not edges or bad:
                return False
    return True


# ---- DXF ----

def dxf_text(path):
    """A DXF file's text as CADCraft reads it: UTF-8 when its bytes are valid UTF-8 (every 2007+ file, and an older one
    that is), otherwise the code page its $DWGCODEPAGE names, in which upstream saves pre-2007 text (a '²' read from
    apartment.dxf is saved as the one byte B2); then the \\U+XXXX escapes for characters outside the code page."""
    b = open(path, 'rb').read()
    try:
        t = b.decode('utf-8')
    except UnicodeDecodeError:
        lines = b.decode('latin-1').replace('\r\n', '\n').split('\n')
        cp = next((lines[i + 2].strip().upper().replace('-', '_') for i in range(len(lines) - 2)
                   if lines[i].strip() == '$DWGCODEPAGE'), '')
        name = ('mac_roman' if cp in ('MACINTOSH', 'MAC_ROMAN') else 'cp1252' if cp in ('ASCII', 'UTF8', 'UTF_8', 'UNICODE') else
                re.sub(r'^(ANSI_|DOS)', 'cp', re.sub(r'^ISO_?8859_', 'iso8859_', cp)))
        try:
            codecs.lookup(name)
        except LookupError:
            name = 'cp1252'
        t = b.decode(name, errors='replace')
    t = t.replace('\r\n', '\n').replace('\r', '\n')
    return re.sub(r'(?:\\U\+[0-9A-Fa-f]{4})+', lambda m: b''.join(
        int(h, 16).to_bytes(2, 'big') for h in m.group().split('\\U+')[1:]).decode('utf-16-be', 'replace'), t)


def pairs_of(text):
    lines = text.replace('\r\n', '\n').split('\n')
    return [[lines[i].strip(), lines[i + 1]] for i in range(0, len(lines) - 1, 2)]


def entities(text):
    """[(kind, layer, pairs)] for every entity in the ENTITIES section."""
    p = pairs_of(text)
    out, inside, cur = [], False, None
    for i, (c, v) in enumerate(p):
        if c == '2' and i and p[i - 1] == ['0', 'SECTION']:
            inside = v.strip() == 'ENTITIES'; continue
        if not inside:
            continue
        if c == '0':
            if cur:
                out.append(cur)
            cur = None if v.strip() in ('ENDSEC',) else [v.strip(), '', []]
            if v.strip() == 'ENDSEC':
                inside = False
            continue
        if cur is not None:
            if c == '8' and not cur[1]:
                cur[1] = v.strip()
            cur[2].append((c, v))
    if cur:
        out.append(cur)
    return [tuple(e) for e in out]


def values(pairs, code):
    return [v.strip() for c, v in pairs if c == code]


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


# ---- phase 10 ----

def room_cmds(x0=0.0, y0=0.0, w=4.0, h=4.0, height=2.4):
    """Exterior walls (0.2 thick) whose inner faces are the w x h room at (x0, y0), then roomdetect: the room is r0."""
    a, b = x0 - 0.1, y0 - 0.1
    c, d = x0 + w + 0.1, y0 + h + 0.1
    return [cmd('wall', {"points": [[a, b], [c, b], [c, d], [a, d], [a, b]], "thickness": 0.2, "height": height, "exterior": True}),
            'roomdetect {}']


COLUMN = cmd('wall', {"points": [[1.4, 1.5], [1.6, 1.5]], "thickness": 0.2, "height": 1.0, "exterior": False})   # 0.2 x 0.2 x 1.0 at (1.5, 1.5)
POTS = cmd('lightfixture', {"room": "r0", "rows": 2, "cols": 2})


def lights_of(rows, nth=-1):
    """The fixtures listed by an `arch.inspect {"lights":true}` reply (a list of objects; `lightfixture` replies list labels)."""
    hits = [r['lights'] for r in rows if isinstance(r, dict) and isinstance(r.get('lights'), list)
            and all(isinstance(x, dict) for x in r['lights']) and 'handles' not in r]
    if not hits or not (-len(hits) <= nth < len(hits)):
        fail(f"no arch.inspect reply listing lights; replies: {json.dumps(rows)[:600]}")
    return hits[nth]


def placed(rows, nth=-1):
    """The labels a `lightfixture` reply placed."""
    hits = [r for r in rows if isinstance(r, dict) and 'handles' in r and isinstance(r.get('lights'), list)]
    if not hits or not (-len(hits) <= nth < len(hits)):
        fail(f"no lightfixture reply; replies: {json.dumps(rows)[:600]}")
    return hits[nth]['lights']


def renders(rows):
    return [r for r in rows if isinstance(r, dict) and r.get('ok') is True and 'shadowMapsBuilt' in r and 'samples' in r]


def rgb_sum(sample, what):
    if not isinstance(sample, dict) or not isinstance(sample.get('rgb'), list) or len(sample['rgb']) < 3:
        fail(f"{what}: no colour in sample {sample!r}")
    if sample.get('visible') is not True:
        fail(f"{what}: the point is not visible from the camera: {sample!r}")
    return sum(num(c, what) for c in sample['rgb'][:3])


def artifact(path, name):
    art = os.environ.get('CHECK_ARTIFACTS_DIR')
    if art and os.path.exists(path):
        os.makedirs(art, exist_ok=True)
        with open(path, 'rb') as s, open(os.path.join(art, name), 'wb') as d:
            d.write(s.read())
