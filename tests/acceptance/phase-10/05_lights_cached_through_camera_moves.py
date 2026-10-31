# Shadow maps are built when the scene or the lights change, never because the camera moved; the interactive budget
# shadows the nearest and brightest 8.
# needs: p10-arch-light-placement, p10-arch3d-scene-lights, p10-arch3d-fixture-meshes, p10-cmd-lightfixture, p10-r3d-light-pack, p10-r3d-shadow-atlas, p10-r3d-lit-pipeline, p10-r3d-lit-frame, p10-r3d-finalize, p10-cmd-render, p10-cli-render-hook
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())


def orbit(az):
    return {"kind": "orbit", "target": [2, 2, 1], "azimuth": az, "elevation": 35, "distance": 12, "fov": 50}


steps = [cmd('render', {"size": [160, 120], "camera": orbit(30 + k), "sun": False}) for k in range(31)]
rc, rr, so, se = run(room_cmds() + [POTS] + steps + ['build3d {}',
                     cmd('lightfixture.edit', {"id": "lf0", "at": [1.2, 1.0]}),
                     cmd('render', {"size": [160, 120], "camera": orbit(61), "sun": False}),
                     cmd('render', {"size": [160, 120], "camera": orbit(62), "sun": False})])
r = renders(rr)
if len(r) != 33:
    fail(f"33 render replies, got {len(r)}")
first = r[0]
if num(first.get('shadowMapsBuilt'), 'shadowMapsBuilt') != 4 or num(first.get('frameShadowMaps'), 'frameShadowMaps') != 4:
    fail(f"the first frame builds one map per pot light: shadowMapsBuilt {first.get('shadowMapsBuilt')}, frame {first.get('frameShadowMaps')}")
moved = r[1:31]
if any(num(x.get('frameShadowMaps'), 'frameShadowMaps') != 0 or num(x.get('shadowMapsBuilt'), 'shadowMapsBuilt') != 4
       or num(x.get('finalRenders'), 'finalRenders') != 0 for x in moved):
    fail(f"thirty single-step camera moves build no shadow map and no final render: "
         f"{[(x.get('frameShadowMaps'), x.get('shadowMapsBuilt'), x.get('finalRenders')) for x in moved]}")
b = reply_with(rr, 'fixtures')
if num(b.get('shadowMapsBuilt'), 'build3d shadowMapsBuilt') != 4 or num(b.get('finalRenders'), 'build3d finalRenders') != 0:
    fail(f"build3d reports the session's shadowMapsBuilt 4 and finalRenders 0: {b.get('shadowMapsBuilt')}, {b.get('finalRenders')}")
after = r[31]
if num(after.get('frameShadowMaps'), 'frameShadowMaps') != 1 or num(after.get('shadowMapsBuilt'), 'shadowMapsBuilt') != 5:
    fail(f"moving lf0 rebuilds only lf0's map: frame {after.get('frameShadowMaps')}, total {after.get('shadowMapsBuilt')} (want 1, 5)")
if num(r[32].get('frameShadowMaps'), 'frameShadowMaps') != 0:
    fail("the camera move after the fixture move builds nothing")
print("ok: 4 maps on the first frame, none in 30 camera moves, 1 after moving lf0")

rc, rr, so, se = run(room_cmds(w=6.0, h=4.0) + [cmd('lightfixture', {"room": "r0", "rows": 3, "cols": 4}),
                                                cmd('render', {"size": [160, 120], "camera": orbit(30), "sun": False}),
                                                cmd('render', {"size": [160, 120], "camera": orbit(30), "sun": False, "budget": 2})])
r = renders(rr)
if num(r[0].get('lights'), 'lights') != 12 or num(r[0].get('shadowedLights'), 'shadowedLights') != 8 or num(r[0].get('frameShadowMaps'), 'f') != 8:
    fail(f"12 lights over the budget of 8: 8 shadowed, the rest unshadowed: {r[0].get('lights')}, {r[0].get('shadowedLights')}, {r[0].get('frameShadowMaps')}")
if num(r[1].get('shadowedLights'), 'shadowedLights') != 2:
    fail(f"budget 2 shadows 2 lights: {r[1].get('shadowedLights')}")
print("ok: 12 lights, budget 8 -> 8 shadowed; budget 2 -> 2")
print("PASS 05")
