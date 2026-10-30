# Housekeeping first (phase-7 gaps), plus the two save-side facts detection depends on:
# - docs/arch3d/USER-GUIDE.md runs: every ```sh block executes as written (the guide still showed an invented
#   `cadcraft run --command ... --params` syntax after phase 7), its title is not "Phase 6", 51.05 N is Calgary;
# - docs/arch3d/README.md describes the current state; the orphan docs/arch3d/tests.rs is gone; uk.tsv says "Двері";
# - a drawing with arch objects saves $INSUNITS 6 (metres, the arch model's unit; decision 24);
# - the opening proxies on save carry the whole plan symbol (jambs, leaf, swing, frame, glass), so a plan opened in
#   another program, or with the arch record stripped, still shows the doors and windows.
import os, re, shutil
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())
ROOT = os.getcwd()

uk = open('crates/ui-egui/src/i18n/uk.tsv', encoding='utf-8').read()
if re.search(r'\tDoor\tДверь\s*$', uk, re.M) or not re.search(r'\tDoor\tДвері\s*$', uk, re.M):
    fail('uk.tsv must translate "Door" as the Ukrainian "Двері" (it has the Russian "Дверь")')
if os.path.exists('docs/arch3d/tests.rs'):
    fail('docs/arch3d/tests.rs is an orphan (nothing compiles it, and it asserts ARCH-10 is open): delete it')
readme = open('docs/arch3d/README.md', encoding='utf-8').read()
low = readme.lower()
if 'after phase 1' in low or any(w not in low for w in ('user-guide.md', 'design.md', 'elevation', 'section', 'viewsheet', 'archdetect')):
    fail('docs/arch3d/README.md must describe the current state: link USER-GUIDE.md and DESIGN.md and name elevation, '
         'section, viewsheet and archdetect (it still describes the state after phase 1)')
print("ok: uk.tsv, orphan, README")

guide = open('docs/arch3d/USER-GUIDE.md', encoding='utf-8').read()
title = guide.lstrip().split('\n', 1)[0]
if 'phase 6' in title.lower():
    fail(f"USER-GUIDE.md title still says phase 6: {title!r}")
for ln in guide.splitlines():
    if '51.05' in ln and 'london' in ln.lower():
        fail(f"51.05 N is Calgary, not London: {ln.strip()!r}")
if '--command' in guide or '--params' in guide:
    fail("USER-GUIDE.md uses an invented `--command ... --params` syntax: the CLI is `cargo run -q -p cadcraft-cli -- run --metric --cmd '<id> <json>'`")
blocks = re.findall(r'```sh\n(.*?)```', guide, re.S)
joined = '\n'.join(blocks)
for need in ('elevation', 'viewsheet', 'exportpdf', 'archdetect', 'examples/apartment.dxf'):
    if need not in joined:
        fail(f"USER-GUIDE.md needs a runnable ```sh example using `{need}` (phases 7 and 8)")
tmp = tempfile.mkdtemp()
os.symlink(os.path.join(ROOT, 'examples'), os.path.join(tmp, 'examples'))
exe = os.environ.get('CADCRAFT_CLI')
for i, b in enumerate(blocks):
    if 'cadcraft-cli' not in b:
        continue
    if exe:
        script = b.replace('cargo run -q -p cadcraft-cli --', exe).replace('cargo run -p cadcraft-cli --', exe)
    else:
        mf = os.path.join(ROOT, 'Cargo.toml')
        script = b.replace('cargo run -q -p cadcraft-cli --', f'cargo run -q --manifest-path {mf} -p cadcraft-cli --') \
                  .replace('cargo run -p cadcraft-cli --', f'cargo run -q --manifest-path {mf} -p cadcraft-cli --')
    p = subprocess.run(['bash', '-c', 'set -eo pipefail\n' + script], cwd=tmp, capture_output=True, text=True, timeout=900)
    out = p.stdout + p.stderr
    if p.returncode != 0 or 'panicked' in out or re.search(r'^error', out, re.M):
        fail(f"USER-GUIDE.md sh block {i + 1} does not run as written (exit {p.returncode}):\n{b.strip()[:600]}\n--- output ---\n{out[-800:]}")
for f in os.listdir(tmp):
    if f.endswith(('.png', '.pdf')):
        keep_artifact(os.path.join(tmp, f), 'guide-' + f)
print(f"ok: USER-GUIDE.md runs ({len(blocks)} sh blocks)")

# Saved arch drawings are metre drawings, and their opening proxies are the whole symbol.
tmp2 = tempfile.mkdtemp()
path = os.path.join(tmp2, 'h.dxf')
run(['wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}',
     'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}',
     'window {"wall":"w0","offset":7.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}'], save=path)
text = open(path).read()
if header_value(text, '$INSUNITS') != '6':
    fail(f"a drawing with arch objects must save $INSUNITS 6 (metres, the arch model's unit), got {header_value(text, '$INSUNITS')}")
plain_path = os.path.join(tmp2, 'plain.dxf')
open(plain_path, 'w').write(strip_arch(text))
p = pairs_of(open(plain_path).read())
ents, cur = [], None
for name, s, e in sections(p):
    if name == 'ENTITIES':
        for k in range(s, e):
            c, v = p[k]
            if c == '0':
                cur = {'type': v.strip(), 'layer': None, 'n': 0, 'bulge': False}; ents.append(cur)
            elif cur is not None and c == '8':
                cur['layer'] = v.strip()
            elif cur is not None and c == '10':
                cur['n'] += 1
            elif cur is not None and c == '42' and abs(float(v)) > 1e-9:
                cur['bulge'] = True
door_e = [e for e in ents if e['layer'] == 'A-DOOR']
glaz_e = [e for e in ents if e['layer'] == 'A-GLAZ']
if len(door_e) < 4 or not any(e['type'] == 'ARC' or e['bulge'] or e['n'] >= 8 for e in door_e):
    fail(f"a hinged door must save its whole plan symbol (two jambs, the leaf and the swing arc) on A-DOOR; "
         f"found {[(e['type'], e['n']) for e in door_e]}")
if len(glaz_e) < 5:
    fail(f"a slider window must save its whole plan symbol (jambs, frame lines, two glass lines) on A-GLAZ; found {[(e['type'], e['n']) for e in glaz_e]}")
print(f"ok: $INSUNITS 6; plain linework keeps {len(door_e)} door and {len(glaz_e)} window entities")

a = json.load(open(os.path.join(HERE, 'apartment.truth.json')))
fx = 'tests/fixtures/archdetect/apartment.truth.json'
if not os.path.isfile(fx) or json.load(open(fx)) != a:
    fail(f"{fx} must be the apartment truth, identical to tests/acceptance/phase-8/apartment.truth.json")
print("PASS 02")
