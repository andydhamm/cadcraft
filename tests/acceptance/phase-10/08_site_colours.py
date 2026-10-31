# Phase-9 carry-over: the site renders in its own colours — green terrain, asphalt and concrete paths, green crowns.
# needs: p10-cmd-render, p10-cli-render-hook, p10-r3d-lit-pipeline, p10-r3d-lit-frame, p10-r3d-light-pack, p10-r3d-shadow-atlas, p10-r3d-finalize
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())


def rgb(sample, what):
    if not isinstance(sample, dict) or not isinstance(sample.get('rgb'), list) or len(sample['rgb']) < 3:
        fail(f"{what}: no colour in sample {sample!r}")
    return [num(c, what) for c in sample['rgb'][:3]]


SITE = [[0, 0], [40, 0], [40, 30], [0, 30]]
CAM = {"kind": "orbit", "target": [20, 15, 0], "azimuth": 0, "elevation": 89, "distance": 60, "fov": 50}
tmp = tempfile.mkdtemp(); out = os.path.join(tmp, 'site.png')
rc, rr, so, se = run([cmd('terrain', {"boundary": SITE, "points": [[0, 0, 0], [40, 0, 0], [40, 30, 0], [0, 30, 0]]}),
                      cmd('terrain.modify', {"path": {"kind": "driveway", "polyline": [[18, 0], [18, 12]], "width": 3}}),
                      cmd('terrain.modify', {"path": {"kind": "road", "polyline": [[0, 26], [40, 26]], "width": 6}}),
                      cmd('plant', {"species": "Sugar Maple", "at": [8, 18], "age": 30}),
                      cmd('render', {"out": out, "size": [400, 300], "camera": CAM,
                                     "samples": [[32, 10, 0], [18, 5, 0], [30, 26, 0], [8, 18, 0]]})])
r = renders(rr)
if not r:
    fail(f"no render reply: {json.dumps(rr)[-600:]}")
s = r[-1]['samples']
grass, drive, road, crown = (rgb(s[k], w) for k, w in enumerate(['terrain', 'driveway', 'road', 'maple crown']))
print(f"terrain {grass}, concrete driveway {drive}, asphalt road {road}, maple crown {crown}")
if not (grass[1] > grass[0] + 8 and grass[1] > grass[2] + 8):
    fail(f"the terrain renders green, not grey: {grass}")
if not (crown[1] > crown[0] + 8 and crown[1] > crown[2] + 8):
    fail(f"a crown renders green: {crown}")
for name, c in (('driveway', drive), ('road', road)):
    if max(c) - min(c) > 25:
        fail(f"the {name} renders grey: {c}")
if sum(drive) < sum(road) + 60:
    fail(f"concrete renders lighter than asphalt (by 60+ in r+g+b): concrete {drive}, asphalt {road}")
if abs(sum(drive) - sum(grass)) < 20 and max(grass) - min(grass) < 15:
    fail("the driveway and the terrain render alike: the site is still grey")
artifact(out, 'site-colours.png')
print("PASS 08")
