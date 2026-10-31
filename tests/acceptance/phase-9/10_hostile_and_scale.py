# Hostile input never panics and never half-applies; a 5,000-point survey answers quickly.
# needs: p9-cmd-terrain, p9-cmd-terrain-modify, p9-cmd-terrain-query, p9-arch-terrain-csv, p9-cmd-plant, p9-cmd-site-inspect, p9-arch3d-terrain-solid
import os, random, time
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

def refused(cmds, what, check):
    rc, rr, so, se = run(cmds + ['arch.inspect {"terrain":true,"plants":true,"site":true}'], expect_ok=False)
    if rc != 0 and not rr and "invalid value" in se and "--cmd" in se:
        print(f"ok: {what}: refused by the command line"); return
    last = reply_with(rr, 'terrain', -1)
    if not was_refused(rc, se) or not check(last):
        fail(f"{what} must be refused and change nothing: rc {rc}, {json.dumps(last)[:300]} {(so + se)[-200:]}")
    print(f"ok: {what}: refused, nothing written")

none = lambda r: r.get('terrain') is None and not r.get('plants')
refused([cmd('terrain', {"boundary": [[0, 0], [10, 10], [10, 0], [0, 10]]})], 'self-crossing boundary', none)
refused([cmd('terrain', {"boundary": [[0, 0], [1, 0]]})], 'two-point boundary', none)
refused(['terrain {"boundary":[[0,0],[10,0],[10,10]],"points":[[1,1,"x"]]}'], 'non-numeric point', none)
refused([cmd('terrain', {"boundary": SITE, "points": [[1, 1, 1]] * 20001})], '20,001 points', none)
refused([cmd('terrain', {"boundary": SITE, "contours": [0, 1]})], 'zero contour interval', none)
refused([cmd('terrain', {"boundary": SITE, "contours": [1, 0.5]})], 'major below minor', none)
refused([cmd('terrain', {"boundary": SITE, "csvText": "1,2,nan\n"})], 'NaN in CSV', none)
refused([cmd('terrain.modify', {"hill": {"center": [1, 1], "height": 1, "radius": 2}})], 'modifier without a terrain', none)
one = lambda r: isinstance(r.get('terrain'), dict) and not (r['terrain'].get('modifiers') or [])
for bad in ({"hill": {"center": [1, 1], "height": 1, "radius": 0}}, {"hill": {"center": [1, 1], "height": 1e9, "radius": 2}},
            {"flat": {"polygon": [[0, 0], [1, 1]], "elevation": 1}}, {"path": {"kind": "runway", "polyline": [[0, 0], [5, 0]], "width": 3}},
            {"path": {"kind": "road", "polyline": [[0, 0], [5, 0]], "width": -3}}, {"delete": "tm9"}, {"flat": {}, "hill": {}}):
    refused([terrain_cmd(), cmd('terrain.modify', bad)], f"terrain.modify {json.dumps(bad)[:60]}", one)
for bad in ({"species": "Nonesuch", "at": [0, 0]}, {"species": "Red Oak", "at": [0, 0], "age": -1},
            {"species": "Red Oak", "at": [0, 0], "plantedHeight": 99}, {"species": "Red Oak"}):
    refused([cmd('plant', bad)], f"plant {json.dumps(bad)}", none)
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.query', {"points": [[1, 1]] * 1025})], expect_ok=False)
if rc == 0:
    fail("terrain.query with 1,025 points must be refused")

# Random modifier sequences never panic and leave a queryable surface.
rnd = random.Random(9)
for trial in range(6):
    cmds = [terrain_cmd()]
    for _ in range(12):
        k = rnd.choice(['flat', 'hill', 'valley', 'path'])
        c = [rnd.uniform(-5, 45), rnd.uniform(-5, 35)]
        if k == 'flat':
            cmds.append(cmd('terrain.modify', {"flat": {"polygon": [[c[0] + rnd.uniform(-9, 9), c[1] + rnd.uniform(-9, 9)] for _ in range(rnd.randint(3, 6))],
                                                         "elevation": rnd.uniform(-3, 3), "margin": rnd.choice([0, 0.5, 3])}}))
        elif k in ('hill', 'valley'):
            cmds.append(cmd('terrain.modify', {k: {"center": c, ("height" if k == 'hill' else "depth"): rnd.uniform(0.1, 4), "radius": rnd.uniform(0.5, 12)}}))
        else:
            cmds.append(cmd('terrain.modify', {"path": {"kind": "road", "polyline": [c, [c[0] + rnd.uniform(-20, 20), c[1] + rnd.uniform(-20, 20)]], "width": rnd.uniform(0.5, 6)}}))
    cmds += ['terrain.contours {}', 'terrain.volumes {}', cmd('terrain.query', {"points": [[20, 15]]}), 'build3d {}']
    rc, rr, so, se = run(cmds, expect_ok=False)
    if not any(isinstance(r, dict) and 'elevations' in r for r in rr):
        fail(f"trial {trial}: the session must survive random modifiers and still answer terrain.query: {(so + se)[-300:]}")
print("ok: random modifier sequences never panic")

# Scale: a 5,000-point survey CSV on a 200 x 200 m site, release build.
tmp = tempfile.mkdtemp(); csvp = os.path.join(tmp, 'survey.csv')
with open(csvp, 'w') as f:
    f.write("x,y,z\n")
    for i in range(5000):
        x, y = rnd.uniform(0, 200), rnd.uniform(0, 200)
        f.write(f"{x:.3f},{y:.3f},{2 * math.sin(x / 23) + math.cos(y / 17):.3f}\n")
t0 = time.time()
rc, rr, so, se = run([cmd('terrain', {"boundary": [[0, 0], [200, 0], [200, 200], [0, 200]], "csv": csvp}), 'terrain.contours {}',
                      cmd('terrain.query', {"points": [[100, 100]]})], release=True, timeout=1800)
dt = time.time() - t0
if reply_with(rr, 'terrain')['points'] != 5000 or elevations(rr)[0] is None:
    fail("the 5,000-point survey must load and answer a query")
if dt > 10:
    fail(f"5,000 survey points, contours and a query took {dt:.1f} s (bar 10 s, release build)")
print(f"ok: 5,000-point survey + contours + query in {dt:.1f} s")
print("PASS 10")
