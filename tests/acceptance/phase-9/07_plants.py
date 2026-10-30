# Plants: a catalogue of at least 12 species, growth with age, user species from JSON, plan symbol and 3D form.
import os
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'p9lib.inc')).read())

rc, rr, so, se = run(['plant.species {}'])
sp = reply_with(rr, 'species')['species']
forms = {s.get('form') for s in sp if s.get('builtin')}
if len([s for s in sp if s.get('builtin')]) < 12 or forms != {'deciduous-tree', 'evergreen-tree', 'shrub', 'ground-cover'}:
    fail(f"the catalogue needs at least 12 species covering every form: {len(sp)} species, forms {forms}")
spruce = [s for s in sp if s.get('name') == 'Colorado Blue Spruce']
if not spruce or spruce[0].get('crown') != 'cone' or num(spruce[0].get('matureHeight'), 'h') != 15 or num(spruce[0].get('matureSpread'), 's') != 5:
    fail(f"Colorado Blue Spruce: cone, 15 m high, 5 m spread: {spruce}")
print(f"ok: {len(sp)} species, every form")

tmp = tempfile.mkdtemp(); dxf = os.path.join(tmp, 'plants.dxf'); out = os.path.join(tmp, 'plants.glb')
rc, rr, so, se = run([cmd('plant', {"species": "Colorado Blue Spruce", "at": [10, 10], "age": 30}),
                      cmd('plant', {"species": "colorado blue spruce", "at": [20, 10]}),
                      cmd('plant', {"species": "Colorado Blue Spruce", "at": [30, 10], "age": 15}),
                      'arch.inspect {"plants":true}', cmd('build3d', {"probe": [[10, 10, 10], [10, 10, 15.2], [12.6, 10, 3]]})], save=dxf)
ps = [r for r in rr if isinstance(r, dict) and 'plant' in r and 'height' in r]
if [p['plant'] for p in ps] != ['pl0', 'pl1', 'pl2']:
    fail(f"plants are pl0, pl1, pl2: {ps}")
for p, (h, s) in zip(ps, [(15, 5), (1.5, 0.5), (8.25, 2.75)]):
    near(p['height'], h, 1e-9, f"{p['plant']} height"); near(p['spread'], s, 1e-9, f"{p['plant']} spread")
b = probes(rr)
if num(b.get('plants'), 'build3d plants') != 3 or [part(h) for h in b['probe']] != ['plant', None, None]:
    fail(f"three plants in 3D, the mature spruce's crown at (10,10,10): plants {b.get('plants')}, probes {b['probe']}")
cli('convert', [dxf, out, '--no-detect'])
j, binc = glb(out)
lo, hi = node_box(j, 'plant-pl0')
if abs(hi[2] - 15) > 1e-3 or abs((hi[0] - lo[0]) - 5) > 1e-3 or abs((hi[1] - lo[1]) - 5) > 1e-3:
    fail(f"the mature spruce's crown is 15 m to the top and 5 m across: box {lo}..{hi}")
node_box(j, 'plant-pl0-trunk')
text = open(dxf).read()
circ = [e for e in entities(text) if e[0] == 'CIRCLE' and e[1] == 'L-PLNT']
radii = sorted(round(float(values(e[2], '40')[0]), 6) for e in circ)
if radii != [0.25, 1.375, 2.5]:
    fail(f"each plant's plan symbol is a circle at its spread on L-PLNT (radii 0.25, 1.375, 2.5): {radii}")
print("ok: spruce grows 1.5 -> 8.25 -> 15 m; crown and trunk in the GLB; plan circles at the spread")

sp_file = os.path.join(tmp, 'species.json')
json.dump([{"name": "Saskatoon", "form": "shrub", "crown": "sphere", "matureHeight": 4, "matureSpread": 3,
            "plantedHeight": 1, "yearsToMature": 10}], open(sp_file, 'w'))
rc, rr, so, se = run([cmd('plant.species', {"file": sp_file}), cmd('plant', {"species": "Saskatoon", "at": [5, 5], "age": 5})])
p = reply_with(rr, 'plant', -1)
near(p['height'], 2.5, 1e-9, 'user species height at half its years')
rc, rr, so, se = run([cmd('plant.species', {"add": [{"name": "Red Oak", "form": "shrub", "crown": "sphere", "matureHeight": 1,
                                                       "matureSpread": 1, "plantedHeight": 0.5, "yearsToMature": 2}]}),
                      cmd('plant', {"species": "Nonesuch", "at": [0, 0]}), 'arch.inspect {"plants":true}'], expect_ok=False)
if rc == 0 or reply_with(rr, 'plants', -1)['plants']:
    fail("a user species named like a built-in one, and an unknown species, must be refused with nothing written")
print("ok: user species from a JSON file; duplicates of built-ins and unknown species refused")
print("PASS 07")
