# Hostile input: refused, never a panic, never a half-placed grid or a half-applied edit.
# needs: p10-cmd-lightfixture, p10-cmd-illuminance, p10-cmd-render, p10-cmd-sunstudy, p10-arch-light-placement
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())

bad = [
    cmd('lightfixture', {"at": [1, 1], "lumens": -5}),
    cmd('lightfixture', {"at": [1, 1], "lumens": 1e9}),
    cmd('lightfixture', {"at": [1, 1], "kelvin": 0}),
    cmd('lightfixture', {"at": [1, 1], "cone": 0}),
    cmd('lightfixture', {"at": [1, 1], "cone": 200}),
    cmd('lightfixture', {"at": [1, 1], "cone": 60, "falloff": 40}),
    cmd('lightfixture', {"at": [1, 1], "tilt": 400}),
    cmd('lightfixture', {"at": ["a", 1]}),
    cmd('lightfixture', {"at": [1, 1], "type": "Disco ball"}),
    cmd('lightfixture', {"room": "r9", "rows": 2, "cols": 2}),
    cmd('lightfixture', {"room": "r0", "rows": 0, "cols": 2}),
    cmd('lightfixture', {"room": "r0", "rows": 2, "cols": 1000}),
    cmd('lightfixture', {"room": "r0", "rows": 2, "cols": 2, "lumens": -1}),
    cmd('lightfixture', {"at": [1, 1], "room": "r0"}),
    cmd('lightfixture.edit', {"id": "lf99", "lumens": 10}),
    cmd('lightfixture.edit', {"id": "lf0", "lumens": 10, "kelvin": -3}),
    cmd('lightfixture.edit', {"id": "lf0", "at": [1, "x"]}),
    cmd('lightfixture.delete', {"id": "w0"}),
    cmd('illuminance', {}),
    cmd('illuminance', {"point": [1, 2]}),
    cmd('illuminance', {"point": [1, 1, 0], "normal": [0, 0, 0]}),
    cmd('render', {"size": [0, 0]}),
    cmd('render', {"size": [9000, 10]}),
    cmd('render', {"budget": 65}),
    cmd('render', {"samples": [[0, 0, 0]] * 65}),
    cmd('render', {"sun": {"date": "2026-13-40", "time": "12:00"}}),
    cmd('render', {"camera": {"kind": "full", "eye": [1, 1, 1], "target": [1, 1, 1]}}),
    cmd('sunstudy', {"date": "2026-06-21", "from": "18:00", "to": "08:00", "step": "1h", "out": "x"}),
    cmd('sunstudy', {"date": "2026-06-21", "from": "08:00", "to": "18:00", "step": "0h", "out": "x"}),
]
before = room_cmds() + [POTS, 'arch.inspect {"lights":true}']
rc, rr, so, se = run(before + bad + ['arch.inspect {"lights":true}'], expect_ok=True)
errs = [l for l in se.splitlines() if l.startswith('error: ')]
if len(errs) < len(bad):
    fail(f"each of the {len(bad)} hostile steps is refused with an error line; got {len(errs)}: {errs[-5:]}")
a, b = lights_of(rr, 0), lights_of(rr, -1)
if a != b or len(a) != 4:
    fail(f"refused steps change nothing: {len(a)} fixtures before, {len(b)} after, equal: {a == b}")
print(f"ok: {len(bad)} hostile steps refused, the four fixtures unchanged")

rc, rr, so, se = run(room_cmds() + [cmd('lightfixture', {"room": "r0", "rows": 32, "cols": 32}),
                                    cmd('lightfixture', {"at": [1, 1]}), 'arch.inspect {"lights":true}'])
n = len(lights_of(rr))
if n != 1024 or not any(l.startswith('error: ') for l in se.splitlines()):
    fail(f"1024 fixtures fit; the 1025th is refused: {n} fixtures")
print("ok: the fixture cap holds at 1024")
print("PASS 09")
