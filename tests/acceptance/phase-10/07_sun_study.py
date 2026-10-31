# Sun studies: stills through the day from the current camera with the site's latitude and true north.
# needs: p10-cmd-sunstudy, p10-cmd-render, p10-cli-render-hook, p10-r3d-lit-pipeline, p10-r3d-lit-frame, p10-r3d-finalize, p10-r3d-light-pack, p10-r3d-shadow-atlas
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p10lib.inc')).read())

tmp = tempfile.mkdtemp(); prefix = os.path.join(tmp, 'study', 'house')
CAM = {"kind": "orbit", "target": [5, 4, 1], "azimuth": 215, "elevation": 35, "distance": 30, "fov": 50}
house = [cmd('wall', {"points": [[0, 0], [10, 0], [10, 8], [0, 8], [0, 0]], "thickness": 0.2, "height": 2.4, "exterior": True})]
rc, rr, so, se = run(house + [cmd('site', {"latitude": 51.05, "longitude": -114.07, "trueNorth": 0}),
                              cmd('sunstudy', {"date": "2026-06-21", "from": "08:00", "to": "18:00", "step": "1h", "out": prefix,
                                               "size": [160, 120], "camera": CAM})])
st = reply_with(rr, 'stills')
stills = st['stills']
times = [s.get('time') for s in stills]
want = [f"{h:02d}:00" for h in range(8, 19)]
if times != want or num(st.get('count'), 'count') != 11:
    fail(f"08:00 to 18:00 every hour is 11 stills: {times}, count {st.get('count')}")
az = [num(s.get('azimuth'), 'azimuth') for s in stills]
el = [num(s.get('elevation'), 'elevation') for s in stills]
if any(b <= a for a, b in zip(az, az[1:])):
    fail(f"the sun moves east to west through the day (azimuth rises): {az}")
if max(range(11), key=lambda k: el[k]) != 4 or el[4] < 62.0 or el[4] > 62.8:
    fail(f"the sun is highest at 12:00 solar time, 62.4° at 51.05 N on 06-21: {el}")
files = [s.get('file') for s in stills]
digests = set()
for f in files:
    if not f or not os.path.exists(f):
        fail(f"every still is written: {f}")
    w, h, nch, rows = png(f)
    if (w, h) != (160, 120):
        fail(f"{f} is 160 x 120, got {w} x {h}")
    digests.add(hash(b''.join(rows)))
if len(digests) < 6:
    fail(f"the stills change with the sun (shadows move): only {len(digests)} distinct images of 11")
if not files[0].endswith('house-0800.png') or not files[-1].endswith('house-1800.png'):
    fail(f"stills are named <out>-HHMM.png: {files[0]}, {files[-1]}")
artifact(files[2], 'sunstudy-1000.png'); artifact(files[8], 'sunstudy-1600.png')
print("ok: 11 stills, azimuth rising, highest at noon, the images differ")

rc, rr, so, se = run(house + [cmd('site', {"latitude": 51.05, "trueNorth": 0}),
                              cmd('sunstudy', {"date": "2026-06-21", "from": "02:00", "to": "06:00", "step": "4h", "out": prefix + '-night',
                                               "size": [80, 60], "camera": CAM})])
stills = reply_with(rr, 'stills')['stills']
if [s.get('belowHorizon') for s in stills] != [True, False] or stills[0].get('file') or not stills[1].get('file'):
    fail(f"02:00 is below the horizon (listed, no file), 06:00 is drawn: {stills}")
rc, rr, so, se = run(house + [cmd('sunstudy', {"date": "2026-06-21", "from": "08:00", "to": "18:00", "step": "1h", "out": prefix + '-nosite'})],
                     expect_ok=False)
if not was_refused(rc, se) or os.path.exists(prefix + '-nosite-0800.png'):
    fail("a sun study without a site location is refused and writes nothing")
print("PASS 07")
