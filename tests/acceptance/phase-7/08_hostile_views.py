# Hostile view input is refused (non-zero exit, no "ok"), never clamped, never a panic, and changes nothing.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
PRE = HOUSE + ['elevation {"side":"south","insert":[0,-20]}']
OBS = ['arch.inspect {"views":true}', 'layout.list {}']
_, base_rows, _, _ = run(PRE + OBS)
base = (json.dumps(views_of(base_rows), sort_keys=True), json.dumps(base_rows[-1], sort_keys=True))
bad = [
    'elevation {}', 'elevation {"side":"up"}', 'elevation {"look":[0,0]}', 'elevation {"look":[1e308,1e308]}',
    'elevation {"look":["a",1]}', 'elevation {"side":"south","insert":[1e308,0]}', 'elevation {"side":"south","scale":0}',
    'elevation {"side":"south","scale":-50}', 'elevation {"side":"south","scale":1e9}', 'elevation {"side":"south","look":[0,1]}',
    'section {"from":[1,1],"to":[1,1]}', 'section {"from":[-1,4],"to":[11,4],"look":[1,0]}', 'section {"from":[0,0]}',
    'section {"from":[-1,4],"to":[11,4],"depth":-1}', 'section {"from":[-1,4],"to":[1e308,4]}',
    'viewsheet {"views":["v9"]}', 'viewsheet {"views":[]}', 'viewsheet {"views":["v0"],"paper":"ARCH Z"}',
    'viewsheet {"views":["v0","v0"]}', 'view.delete {"view":"v9"}', 'view.delete {}',
]
for c in bad:
    rc, rows, out, err = run(PRE + [c], expect_ok=False)
    if rc == 0:
        fail(f"accepted: {c} -> rc {rc}, {out.strip().splitlines()[-1:] if out.strip() else ''}")
    _, rows, _, _ = run(PRE + [c] + OBS, expect_ok=False)
    got = (json.dumps(views_of(rows), sort_keys=True), json.dumps(rows[-1], sort_keys=True))
    if got != base:
        fail(f"'{c}' was refused but changed the model or the layouts")
_, rows, _, _ = run(PRE + ['view.delete {"view":"v0"}', 'arch.inspect {"views":true}'])
if views_of(rows):
    fail("view.delete v0 left the view in place")
print(f"ok: {len(bad)} hostile inputs refused with nothing changed; view.delete works")
