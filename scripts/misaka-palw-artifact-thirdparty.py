#!/usr/bin/env python3
"""**Ask a library nobody here wrote whether the artifact says what the answer said.**

The tree already has three checks on derived artifacts and every one of them is ours:

  * the corpus goldens pin `artifact_bytes` and `artifact_hash` from the shipped Rust  — determinism
  * `misaka-palw-derive-stranger.py` recomputes those bytes from the spec in Python    — independence
  * `misaka-palw-artifact-conformance.py` opens the file and enforces the format's own
    invariants with a parser written here                                              — well-formedness

Three checks, one author. They agree, and they agree by construction: the same reading of the same
spec is in all three. What none of them can do is disagree with us. That gap is not theoretical —
it hid a real defect until this script was written. `scene/glb/v1` emitted a material with
`baseColorFactor[3] = 0.5` and no `alphaMode`, and glTF's default mode is OPAQUE, which per §3.9.4
IGNORES the alpha channel. The file was conformant, matched its golden, re-derived byte-identically
in Python, and rendered solid. Only something that knows what the bytes MEAN could object, and the
first thing that did was `pygltflib`.

So this script does the one thing the others cannot: it hands each artifact to the library that
ecosystem actually uses, asks that library for a quantity with meaning — a solid's enclosed volume,
a song's playback duration, a material's blend mode — and compares it against the same quantity
computed independently from the DSL. `mido` for SMF, `pygltflib` for glTF, `numpy-stl` for STL.

It is not in the workspace's dependency tree and must not become one: the point is that these
parsers were written by people who have never seen this repository. Install them yourself:

    python3 -m venv /tmp/artifact-venv && /tmp/artifact-venv/bin/pip install mido pygltflib numpy-stl
    /tmp/artifact-venv/bin/python scripts/misaka-palw-artifact-thirdparty.py <dir-of-artifacts>

A missing library is reported as SKIP and does not fail the run, because a machine without it has
not learned anything either way — but `--require` turns every SKIP into a failure, which is what CI
should pass once the libraries are pinned somewhere. Silence is never a pass here: the summary
always names how many kinds went unchecked.
"""

import glob
import json
import os
import sys


class Disagreement(Exception):
    """A library opened the file and described something the DSL did not ask for."""


def _dsl_beside(path):
    """The `.dsl` the artifact was derived from — `palw-derive` writes it next to the artifact."""
    stem = os.path.basename(path).split(".artifact")[0]
    hits = glob.glob(os.path.join(os.path.dirname(path), stem + "*.dsl"))
    if not hits:
        raise Disagreement(f"no .dsl beside {os.path.basename(path)}; nothing to compare against")
    return json.load(open(hits[0], encoding="utf-8"))


def check_midi(path):
    """mido's own playback length and note count, against the DSL's tick arithmetic."""
    import mido

    dsl = _dsl_beside(path)
    m = mido.MidiFile(path)
    facts = {"format": m.type, "tracks": len(m.tracks), "ppq": m.ticks_per_beat}

    if m.ticks_per_beat != dsl["ppq"]:
        raise Disagreement(f"ppq: DSL says {dsl['ppq']}, mido reads {m.ticks_per_beat}")

    want = sum(len(t["notes"]) for t in dsl["tracks"])
    got = sum(1 for t in m.tracks for msg in t if msg.type == "note_on" and msg.velocity > 0)
    if got != want:
        raise Disagreement(f"notes: DSL writes {want}, mido counts {got} note-on(s)")
    facts["note_ons"] = got

    # the DSL's duration is exact rational arithmetic; mido derives its own from the tempo meta
    # event and the delta times it actually read, so agreement means the tempo track is right too
    end_tick = max(n["onset"] + n["duration"] for t in dsl["tracks"] for n in t["notes"])
    want_s = (end_tick * dsl["tempo_us_per_quarter"]) / (dsl["ppq"] * 1_000_000)
    if abs(m.length - want_s) > 1e-9:
        raise Disagreement(f"duration: DSL says {want_s}s, mido plays {m.length}s")
    facts["seconds"] = round(m.length, 6)
    return facts


def check_stl(path):
    """numpy-stl's enclosed volume, against the shoelace area of the DSL sketch times its height."""
    import numpy as np
    from stl import mesh

    dsl = _dsl_beside(path)
    m = mesh.Mesh.from_file(path)
    facts = {"triangles": len(m.vectors)}

    areas = np.linalg.norm(np.cross(m.v1 - m.v0, m.v2 - m.v0), axis=1) / 2.0
    degenerate = int((areas == 0).sum())
    if degenerate:
        raise Disagreement(f"{degenerate} of {len(m.vectors)} triangles have zero area")

    solid = dsl.get("solid", {})
    if solid.get("op") == "extrude":
        p = dsl["sketches"][solid["sketch"]]
        scale = 2 ** dsl.get("frac_bits", 0)
        shoelace = abs(sum(p[i][0] * p[(i + 1) % len(p)][1] - p[(i + 1) % len(p)][0] * p[i][1]
                           for i in range(len(p)))) / 2 / (scale ** 2)
        want = shoelace * (solid["z1"] - solid["z0"]) / scale
        got = float(m.get_mass_properties()[0])
        if abs(got - want) > 1e-6 * max(1.0, abs(want)):
            raise Disagreement(f"volume: sketch area x height is {want}, numpy-stl encloses {got}")
        facts["volume"] = got
    else:
        facts["volume"] = "not checked: only `extrude` has a closed form here"
    return facts


def check_glb(path):
    """pygltflib's decoded materials and bounds, against the DSL's fixed-point values.

    The alpha rule is the reason this file exists — see the module docstring.
    """
    from pygltflib import GLTF2

    dsl = _dsl_beside(path)
    g = GLTF2().load(path)
    facts = {"meshes": len(g.meshes), "nodes": len(g.nodes), "accessors": len(g.accessors)}

    if len(g.nodes) != _count_nodes(dsl["nodes"]):
        raise Disagreement(f"nodes: DSL has {_count_nodes(dsl['nodes'])}, glTF has {len(g.nodes)}")

    denominator = 256  # scene/glb/v1's CHANNEL_DENOMINATOR; channels are n/256, exactly
    for want, got in zip(dsl["materials"], g.materials):
        alpha = want["base_color"][3]
        mode = got.alphaMode
        if alpha < denominator and mode != "BLEND":
            raise Disagreement(
                f"material {want['name']!r}: the DSL asks for alpha {alpha}/{denominator} and the "
                f"file says alphaMode={mode}, which per glTF 3.9.4 IGNORES alpha — it renders solid")
        if alpha == denominator and mode != "OPAQUE":
            raise Disagreement(f"material {want['name']!r}: full-scale alpha should not blend, got {mode}")
        if got.doubleSided != want["double_sided"]:
            raise Disagreement(f"material {want['name']!r}: doubleSided {got.doubleSided} != {want['double_sided']}")
    facts["blend_materials"] = sum(1 for m in g.materials if m.alphaMode == "BLEND")
    return facts


def _count_nodes(nodes):
    return sum(1 + _count_nodes(n.get("children", [])) for n in nodes)


CHECKS = {".mid": ("mido", check_midi), ".midi": ("mido", check_midi),
          ".glb": ("pygltflib", check_glb), ".stl": ("numpy-stl", check_stl)}


def main(argv):
    require = "--require" in argv
    roots = [a for a in argv if not a.startswith("--")] or ["."]
    if not roots:
        print(__doc__)
        return 2

    files = []
    for r in roots:
        files.extend(sorted(glob.glob(os.path.join(r, "**", "*.*"), recursive=True))
                     if os.path.isdir(r) else [r])
    files = [f for f in files if os.path.splitext(f)[1].lower() in CHECKS]
    if not files:
        print(f"no artifacts with a registered reader under {roots}", file=sys.stderr)
        return 2

    bad = skipped = 0
    for f in files:
        library, fn = CHECKS[os.path.splitext(f)[1].lower()]
        try:
            facts = fn(f)
        except ImportError:
            print(f"SKIP  {os.path.basename(f):<44} {library} is not installed — nothing was learned")
            skipped += 1
            continue
        except Disagreement as e:
            print(f"WRONG {os.path.basename(f):<44} {library} disagrees with the DSL: {e}", file=sys.stderr)
            bad += 1
            continue
        except Exception as e:
            print(f"FAIL  {os.path.basename(f):<44} {library} could not open it: {type(e).__name__}: {e}", file=sys.stderr)
            bad += 1
            continue
        print(f"AGREE {os.path.basename(f):<44} {library}: {json.dumps(facts)}")

    print(f"\n{len(files) - bad - skipped} agreed, {bad} disagreed, {skipped} unchecked"
          + (" (a skip is not a pass)" if skipped else ""))
    if skipped and require:
        print("--require was given and a library was missing", file=sys.stderr)
        return 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
