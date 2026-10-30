# Proof on screen (Andy 2026-10-29): the app itself, driven by egui_kittest and rendered with wgpu (the software
# adapter when there is no GPU; no SKIP), shows apartment.dxf with the Detection Review panel listing the candidates,
# and after "Accept Selected" the 3D pane shows the house. The test crates/ui-egui/tests/archdetect_review.rs writes
# review.png, model3d.png and shots.json ({"review": {"rect": [x0,y0,x1,y1], "rows": n}, "pane3d": {"rect": [...]}},
# pixel rectangles of the panel and of the 3D pane) into CADCRAFT_ARCHDETECT_SHOTS. The images are judged against the
# background (no pixel goldens across adapters) and kept for the dashboard.
import os, shutil
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p8lib.inc')).read())

shots = tempfile.mkdtemp()
p = subprocess.run(['cargo', 'test', '-q', '-p', 'cadcraft-ui-egui', '--test', 'archdetect_review', '--', '--nocapture', '--test-threads=1'],
                   capture_output=True, text=True, timeout=1800, env={**os.environ, 'CADCRAFT_ARCHDETECT_SHOTS': shots})
for f in ('review.png', 'model3d.png'):
    keep_artifact(os.path.join(shots, f))
if p.returncode != 0:
    fail(f"the archdetect_review UI test failed:\n{(p.stdout + p.stderr)[-1500:]}")
try:
    meta = json.load(open(os.path.join(shots, 'shots.json')))
except (OSError, ValueError) as e:
    fail(f"the UI test must write shots.json: {e}")


def region(png_path, rect, what):
    w, h, nch, rows = png(png_path)
    x0, y0, x1, y1 = [int(round(num(v, f'{what} rect'))) for v in rect]
    if not (0 <= x0 < x1 <= w and 0 <= y0 < y1 <= h) or (x1 - x0) < 120 or (y1 - y0) < 120:
        fail(f"{what} rect {rect} is not a sizeable region inside the {w}x{h} image")
    px = [tuple(rows[y][x * nch:x * nch + 3]) for y in range(y0, y1, 2) for x in range(x0, x1, 2)]
    bg = max(set(px), key=px.count)
    fg = [q for q in px if sum(abs(a - b) for a, b in zip(q, bg)) > 24]
    return len(fg) / len(px), len(set(fg)), (w, h)


f, shades, size = region(os.path.join(shots, 'review.png'), meta['review']['rect'], 'review panel')
rows_n = int(num(meta['review'].get('rows'), 'review rows'))
print(f"review.png {size}: panel foreground {f:.1%}, {rows_n} candidate rows")
if rows_n < 10:
    fail(f"the review panel must list apartment.dxf's candidates (6 walls, 7 openings, 3 rooms), it lists {rows_n}")
if f < 0.02:
    fail(f"the review panel region is blank ({f:.1%} foreground)")
f3, shades3, size3 = region(os.path.join(shots, 'model3d.png'), meta['pane3d']['rect'], '3D pane')
print(f"model3d.png {size3}: 3D pane foreground {f3:.1%}, {shades3} shades")
if not 0.05 <= f3 <= 0.95 or shades3 < 8:
    fail(f"the 3D pane must show the lit house ({f3:.1%} foreground, {shades3} shades): a placeholder text or an empty "
         f"or flat pane does not count")
print("PASS 10")
