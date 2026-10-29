# Phase-6 carry-over: arch.inspect, pick3d and the opening commands name an object the way the 2D pane does.
# JSON handles are 0xA000_0000 + the kind's handle offset + id in lower-case hex (the low 32 bits of the proxy
# handle): walls +0, openings +0x8000, rooms +0x10000, roofs +0x18000. Before phase 7 openings reported a wall's
# handle (o0 and w0 both "a0000000"), so a 3D pick of the door named the wall; the wall command answered
# "A0000000" and the door command the raw "ffffffff00008000".
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p7lib.inc')).read())
C = {"kind": "full", "eye": [3, -8, 1.6], "target": [3, 0, 1.05], "fov": 60}
_, rows, _, _ = run(HOUSE + ROOF + [
    'arch.inspect {"walls":true,"openings":true,"rooms":true,"roofs":true}',
    'pick3d ' + json.dumps({"camera": C, "size": [480, 320], "pixel": [240, 160]}),
    'arch.opening.move {"opening":"o0","offset":3.5}',
])
wall_reply = reply_with(rows, 'walls', 0, 'wall reply')
door_reply = reply_with(rows, 'openings', 0, 'door reply')
ins = reply_with(rows, 'openings', -1, 'arch.inspect')
if [w.get('handle') for w in wall_reply.get('walls') or []] != ['a0000000', 'a0000001', 'a0000002', 'a0000003']:
    fail(f"the wall command's handles {[w.get('handle') for w in wall_reply.get('walls') or []]}, want a0000000..a0000003")
if [o.get('handle') for o in door_reply.get('openings') or []] != ['a0008000']:
    fail(f"the door command's handle {[o.get('handle') for o in door_reply.get('openings') or []]}, want ['a0008000']")
got = {}
for key in ('walls', 'openings', 'rooms', 'roofs'):
    for o in ins.get(key) or []:
        got[(key, o.get('id'))] = o.get('handle')
want = {('walls', 'w0'): 'a0000000', ('walls', 'w1'): 'a0000001', ('walls', 'w2'): 'a0000002', ('walls', 'w3'): 'a0000003',
        ('openings', 'o0'): 'a0008000', ('rooms', 'r0'): 'a0010000', ('roofs', 'r0'): 'a0018000'}
for k, h in want.items():
    if got.get(k) != h:
        fail(f"arch.inspect {k[0]} {k[1]} handle {got.get(k)!r}, want {h!r} (0xA000_0000 + kind offset + id)")
hit = reply_with(rows, 'hit', what='pick3d').get('hit')
if not isinstance(hit, dict) or hit.get('id') != 'o0':
    fail(f"the front camera's centre pixel must pick the door o0: {hit}")
if hit.get('handle') != 'a0008000':
    fail(f"pick3d names the door {hit.get('handle')!r}; the 2D pane's door handle is 'a0008000' (w0 is 'a0000000')")
mv = reply_with(rows, 'moved', what='arch.opening.move')
if mv.get('moved') != 'o0':
    fail(f"arch.opening.move must name the opening it moved: moved={mv.get('moved')!r}, want 'o0'")
print("ok: wall/door replies and inspect agree; walls a000000n, door a0008000 (inspect = pick3d), room a0010000, roof a0018000; moved o0")
