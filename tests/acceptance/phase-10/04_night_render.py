# The interior at night: the sun off, the pot lights light their cones on the floor and leave the floor between them dark.
# needs: p10-arch-light-placement, p10-arch3d-scene-lights, p10-arch3d-fixture-meshes, p10-cmd-lightfixture, p10-r3d-light-pack, p10-r3d-shadow-atlas, p10-r3d-lit-pipeline, p10-r3d-lit-frame, p10-r3d-finalize, p10-cmd-render, p10-cli-render-hook
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())

CAM = {"kind": "full", "eye": [2.0, 0.3, 2.1], "target": [2.0, 2.6, 0.0], "fov": 90}
SAMPLES = [[1, 1, 0], [3, 1, 0], [2, 2, 0], [2, 1, 0]]
tmp = tempfile.mkdtemp(); night = os.path.join(tmp, 'night.png'); day = os.path.join(tmp, 'day.png')
rc, rr, so, se = run(room_cmds() + [POTS,
                                    cmd('render', {"out": night, "size": [320, 240], "camera": CAM, "sun": False, "samples": SAMPLES}),
                                    cmd('render', {"out": day, "size": [320, 240], "camera": CAM, "samples": SAMPLES})])
r = renders(rr)
if len(r) != 2:
    fail(f"two render replies, got {len(r)}: {json.dumps(rr)[-600:]}")
n = r[0]
if num(n.get('lights'), 'lights') != 4 or num(n.get('shadowedLights'), 'shadowedLights') != 4:
    fail(f"the night render draws and shadows the four pot lights: lights {n.get('lights')}, shadowed {n.get('shadowedLights')}")
w, h, nch, rows = png(night)
if (w, h) != (320, 240):
    fail(f"night.png is 320 x 240, got {w} x {h}")
lit = [rgb_sum(n['samples'][k], f"night sample {SAMPLES[k]}") for k in range(4)]
# (1,1) and (3,1) are under lf0 and lf2; (2,1) is inside both their cones (22.6° off axis); (2,2) is 30.5° off every axis.
print(f"night floor (r+g+b): under lf0 {lit[0]:.0f}, under lf2 {lit[1]:.0f}, between two cones {lit[3]:.0f}, outside every cone {lit[2]:.0f}")
if min(lit[0], lit[1], lit[3]) < lit[2] + 90:
    fail(f"at night the floor inside the cones is far brighter (by 90+ in r+g+b) than the floor outside every cone: {lit}")
if lit[2] > 120:
    fail(f"with the sun off, the floor outside every cone stays dark (r+g+b <= 120): {lit[2]:.0f}")
d = r[1]
day_between = rgb_sum(d['samples'][2], 'day sample (2,2,0)')
if day_between < lit[2] + 30:
    fail(f"the default daylight lights the floor between the cones more than the night does: day {day_between:.0f}, night {lit[2]:.0f}")
artifact(night, 'night-room.png')
print("PASS 04")
