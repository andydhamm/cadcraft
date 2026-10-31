# Terrain from points, elevation lines (breaklines) and CSV with the site's reference point; queries inside and outside.
# needs: p9-cmd-terrain, p9-cmd-terrain-query, p9-cmd-site-inspect
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

# (a) The 40 x 30 site with its four corners at z = 0.05 x is the plane z = 0.05 x.
rc, rr, so, se = run([terrain_cmd(), cmd('terrain.query', {"points": [[25, 12], [0, 0], [40, 30], [10, 29.5], [41, 5], [-0.5, 10]]}),
                      cmd('terrain.query', {"points": [[25, 12]], "surface": "original"})])
t = reply_with(rr, 'terrain')
if t['terrain'] != 't0':
    fail(f"terrain reply must name t0: {t}")
if num(t.get('triangles'), 'triangles') < 2:
    fail(f"the site needs at least two triangles: {t}")
r = t.get('range')
if not isinstance(r, list) or len(r) != 2:
    fail(f"terrain reply needs range [zmin, zmax]: {t}")
near(r[0], 0.0, 1e-9, 'range min'); near(r[1], 2.0, 1e-9, 'range max')
e = elevations(rr, 0)
for got, want, what in zip(e, [1.25, 0.0, 2.0, 0.5], ['(25,12)', '(0,0)', '(40,30)', '(10,29.5)']):
    near(got, want, 1e-6, f"elevation at {what}")
if e[4] is not None or e[5] is not None:
    fail(f"points outside the boundary must answer null, got {e[4:]}")
near(elevations(rr, 1)[0], 1.25, 1e-6, 'original surface at (25,12)')
print("ok: plane z = 0.05x, 1.25 at (25,12), null outside")

# (b) An elevation line is a breakline: an edge of the TIN, which a plain Delaunay TIN of the same points would cross.
pts = [[0, 0, 0], [40, 0, 0], [40, 30, 0], [0, 30, 0], [20, 13, 0], [20, 17, 0]]
rc, rr, so, se = run([cmd('terrain', {"boundary": SITE, "points": pts, "lines": [{"points": [[2, 15], [38, 15]], "elevation": 3}]}),
                      cmd('terrain.query', {"points": [[20, 15], [20, 14], [2, 15], [38, 15]]})])
e = elevations(rr)
near(e[0], 3.0, 1e-6, 'elevation on the breakline at (20,15)')
near(e[1], 1.5, 1e-6, 'elevation at (20,14), halfway between the breakline and (20,13)')
near(e[2], 3.0, 1e-6, 'breakline end (2,15)'); near(e[3], 3.0, 1e-6, 'breakline end (38,15)')
print("ok: the elevation line is an edge (3.0 on it, 1.5 halfway down)")

# (c) CSV in real-world coordinates, the site's reference point subtracted.
csv = "easting,northing,elevation\n500000,5650000,0\n500040,5650000,2\n500040,5650030,2\n500000,5650030,0\n"
rc, rr, so, se = run([cmd('site', {"latitude": 51.05, "longitude": -114.07,
                                    "reference": {"point": [0, 0], "easting": 500000, "northing": 5650000, "crs": "EPSG:32611"}}),
                      cmd('terrain', {"boundary": SITE, "csvText": csv, "subtractReference": True}),
                      cmd('terrain.query', {"points": [[25, 12]]}), 'arch.inspect {"site":true}'])
near(elevations(rr)[0], 1.25, 1e-6, 'CSV terrain at (25,12)')
s = reply_with(rr, 'site', -1)['site']
if not isinstance(s, dict) or (s.get('reference') or {}).get('crs') != 'EPSG:32611' or num((s.get('reference') or {}).get('easting'), 'easting') != 500000:
    fail(f"site reference must be stored as given: {s}")
print("ok: CSV with the reference easting/northing subtracted")

# (d) A bad CSV row is refused with its line number, and nothing is written.
rc, rr, so, se = run([cmd('terrain', {"boundary": SITE, "csvText": "x,y,z\n1,2,3\n4,5\n6,7,8\n"}), 'arch.inspect {"terrain":true}'], expect_ok=False)
if not was_refused(rc, se) or 'line 3' not in (so + se):
    fail(f"a CSV row with two values must be refused naming line 3: rc {rc}, {(so + se)[-300:]}")
if reply_with(rr, 'terrain', -1).get('terrain') is not None:
    fail("a refused terrain must not be written")
print("ok: bad CSV refused at line 3, nothing written")
print("PASS 01")
