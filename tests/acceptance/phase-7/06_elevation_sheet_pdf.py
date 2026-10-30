# The demo: the four elevations on an ARCH D layout at 1:50, each with an overall width and height dimension, plotted
# through the existing plot path (PDF, and PNG at 50 dpi to measure). viewsheet makes one viewport per view at
# 1000/scale paper mm per model metre (20 at 1:50) and reports them in paper mm from the sheet's lower-left corner.
import os, re
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
tmp = tempfile.mkdtemp()
pdf, pngp = os.path.join(tmp, 'e.pdf'), os.path.join(tmp, 'e.png')
import atexit, shutil
atexit.register(lambda: [shutil.copy(f, os.environ['CHECK_ARTIFACTS_DIR']) for f in (pdf, pngp)
                         if os.environ.get('CHECK_ARTIFACTS_DIR') and os.path.isfile(f)])  # kept by the gate
_, rows, _, _ = run(HOUSE + ROOF + [
    'elevation {"side":"south","insert":[0,-20]}', 'elevation {"side":"east","insert":[16,-20]}',
    'elevation {"side":"north","insert":[0,-32]}', 'elevation {"side":"west","insert":[16,-32]}',
    'viewsheet {"name":"Elevations","views":["v0","v1","v2","v3"],"paper":"ARCH D"}',
    'exportpdf ' + json.dumps({"layout": "Elevations", "path": pdf}),
    'plot ' + json.dumps({"layout": "Elevations", "output": "png", "resolution": 50, "path": pngp}),
    'arch.inspect {"views":true}'])
sheet = reply_with(rows, 'viewports', what='viewsheet')
if sheet.get('layout') != 'Elevations':
    fail(f"viewsheet layout {sheet.get('layout')!r}")
vps = sheet.get('viewports') or []
if [vp.get('view') for vp in vps] != ['v0', 'v1', 'v2', 'v3']:
    fail(f"one viewport per view, in order: {[vp.get('view') for vp in vps]}")
for vp in vps:
    if abs(num(vp.get('scale'), 'viewport scale') - 20.0) > 1e-9:
        fail(f"viewport for {vp.get('view')} at {vp.get('scale')} paper mm per metre, want 20 (1:50)")
views = {v.get('id'): v for v in views_of(rows)}
want_w = {'v0': 11.2, 'v1': 9.2, 'v2': 11.2, 'v3': 9.2}
heights = []
for vid, ww in want_w.items():
    dims = {d.get('kind'): d for d in (views.get(vid) or {}).get('dims') or []}
    if set(dims) != {'width', 'height'}:
        fail(f"{vid}: one overall width and one overall height dimension: {list(dims)}")
    if abs(num(dims['width'].get('value'), 'width') - ww) > 0.005:
        fail(f"{vid}: overall width {dims['width'].get('value')}, want {ww}")
    heights.append(num(dims['height'].get('value'), 'height'))
if max(heights) - min(heights) > 0.005:
    fail(f"the four elevations of one house have different overall heights: {heights}")
# PDF: an ARCH D landscape page carrying the linework.
b = open(pdf, 'rb').read() if os.path.exists(pdf) else b''
if not b.startswith(b'%PDF'):
    fail("exportpdf wrote no PDF")
mb = re.search(rb'/MediaBox\s*\[\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*\]', b)
if not mb or abs(float(mb.group(1)) - 2592) > 1 or abs(float(mb.group(2)) - 1728) > 1:
    fail(f"PDF page is not ARCH D landscape (2592 x 1728 pt): {mb.group(0) if mb else 'no MediaBox'}")
ops = 0
# Each stream is exactly its /Length bytes (10-29: a regex up to 'endstream' cut a FlateDecode stream whose last byte
# was 0x0D, so it would not inflate and a sheet of 349 segments counted as none).
for m in re.finditer(rb'/Length\s+(\d+)[^>]*>>\s*stream\r?\n', b):
    s = b[m.end():m.end() + int(m.group(1))]
    try:
        s = zlib.decompress(s)
    except zlib.error:
        pass
    ops += len(re.findall(rb'\sl\b', s))
if ops < 150:
    fail(f"the PDF draws only {ops} line segments: the elevations are not on the sheet")
# PNG at 50 dpi: the same sheet, 1800 x 1200 px; each viewport holds its elevation at 1:50.
w, h, nch, px = png(pngp)
if abs(w - 1800) > 2 or abs(h - 1200) > 2:
    fail(f"PNG plot is {w} x {h}, want 1800 x 1200 (ARCH D landscape at 50 dpi)")
k = 50 / 25.4
corner = px[2][2 * nch:2 * nch + 3]
if min(corner[:3] if nch >= 3 else corner[:1]) < 200:
    fail(f"PNG background is not white: {list(corner)}")
for vp, vid in zip(vps, want_w):
    c = pt(vp.get('center'), 'viewport center'); vw = num(vp.get('width'), 'width'); vh = num(vp.get('height'), 'height')
    x0, x1 = int((c[0] - vw / 2) * k) + 4, int((c[0] + vw / 2) * k) - 4
    y0, y1 = int(h - (c[1] + vh / 2) * k) + 4, int(h - (c[1] - vh / 2) * k) - 4
    xs, ys = [], []
    for y in range(max(0, y0), min(h, y1)):
        row = px[y]
        for x in range(max(0, x0), min(w, x1)):
            if min(row[x * nch:x * nch + min(nch, 3)]) < 160:
                xs.append(x); ys.append(y)
    if not xs:
        fail(f"viewport for {vid} at {c} mm is empty on the plot")
    iw = (max(xs) - min(xs) + 1) / k; ih = (max(ys) - min(ys) + 1) / k
    lo, hi = want_w[vid] * 20 * 0.97, want_w[vid] * 20 + 80
    if not lo <= iw <= hi:
        fail(f"{vid}: drawn {iw:.1f} mm wide, want {want_w[vid] * 20:.0f} mm (+ its height dimension) at 1:50")
    if ih < 0.9 * heights[0] * 20:
        fail(f"{vid}: drawn {ih:.1f} mm tall, want at least {0.9 * heights[0] * 20:.0f} mm at 1:50")
print(f"ok: 4 viewports at 1:50 on ARCH D; PDF {ops} segments; widths 11.2/9.2/11.2/9.2 m, height {heights[0]:.3f} m")
