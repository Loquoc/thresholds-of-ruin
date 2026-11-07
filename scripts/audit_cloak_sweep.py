#!/usr/bin/env python3
"""Offline diagnostic, not a replacement for native cloth collision tests.

A triangle whose vertices move independently along three bounded segments stays
inside the convex hull of their six endpoints. A unit supporting plane proves
that a witness outside that hull by more than the clearance is irrelevant to
finite arm proximity, NOT to outermost inner-cloth containment. Passing the
plane tests is NOT proof of collision or proximity:
expanded facet intersections conservatively overestimate rounded clearance.
No geometry, manifests, or repository files are modified by this tool.
"""
import argparse
import itertools
import json
import math
from pathlib import Path


def sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0])


def unit(v):
    length = math.sqrt(dot(v, v))
    return tuple(x / length for x in v) if length > 1e-12 else None


def vector(v):
    if not isinstance(v, (list, tuple)) or len(v) != 3:
        raise ValueError("expected three coordinates")
    if any(isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x) for x in v):
        raise ValueError("coordinates must be finite numbers")
    return tuple(float(x) for x in v)


def support_planes(points):
    """Return conservative unit planes n.p <= h, including degenerate hulls.

    Every offset is recomputed as max(n.endpoint); floating facet discovery
    therefore cannot exclude any endpoint. Missing a nearly degenerate facet
    only makes the audit less decisive, never creates a false exclusion.
    """
    points = [vector(p) for p in points]
    if not points:
        raise ValueError("empty hull")
    normals = [(1., 0., 0.), (0., 1., 0.), (0., 0., 1.)]
    face_normals = []
    for a, b, c in itertools.combinations(points, 3):
        n = unit(cross(sub(b, a), sub(c, a)))
        if n is None:
            continue
        distances = [dot(n, sub(p, a)) for p in points]
        if min(distances) >= -1e-9 or max(distances) <= 1e-9:
            face_normals.append(n)
    normals.extend(face_normals)
    # Flat initial sweeps need in-plane edge normals as well as their normal.
    # These additional supporting planes are valid for full-rank hulls too.
    for a, b in itertools.combinations(points, 2):
        for n in face_normals:
            edge = unit(cross(sub(b, a), n))
            if edge is not None:
                normals.append(edge)
    # A collinear hull needs its own perpendicular planes, not just world AABB.
    for a, b in itertools.combinations(points, 2):
        along = unit(sub(b, a))
        if along is None:
            continue
        normals.append(along)
        for basis in [(1., 0., 0.), (0., 1., 0.), (0., 0., 1.)]:
            side = unit(cross(along, basis))
            if side is not None:
                normals.append(side)
    planes = []
    seen = set()
    for n in normals:
        for sign in (-1., 1.):
            signed = tuple(sign * x for x in n)
            key = tuple(round(x, 12) for x in signed)
            if key in seen:
                continue
            seen.add(key)
            planes.append((signed, max(dot(signed, p) for p in points)))
    return planes


def audit(trace, clearance=0.032, tolerance=1e-6):
    if not math.isfinite(clearance) or clearance < 0 or not math.isfinite(tolerance) or tolerance < 0:
        raise ValueError("clearance and tolerance must be finite and nonnegative")
    required = ("face_reference", "face_budget_endpoints", "face_axes", "face_budgets", "face_final")
    missing = [key for key in required if key not in trace]
    if missing:
        return {"status": "missing_evidence", "missing": missing}
    if any(not isinstance(trace[key], list) or len(trace[key]) != 3 for key in required):
        raise ValueError("trace must describe exactly three vertices")
    starts = [vector(p) for p in trace["face_reference"]]
    ends = [vector(p) for p in trace["face_budget_endpoints"]]
    axes = [vector(p) for p in trace["face_axes"]]
    finals = [vector(p) for p in trace["face_final"]]
    for start, end, axis, budget, final in zip(starts, ends, axes, trace["face_budgets"], finals):
        if isinstance(budget, bool) or not isinstance(budget, (int, float)) or not math.isfinite(budget) or budget < 0:
            raise ValueError("invalid travel budget")
        if abs(dot(axis, axis) - 1) > 1e-5:
            raise ValueError("movement axis is not unit length")
        if max(abs(e - (s + a * budget)) for e, s, a in zip(end, start, axis)) > tolerance:
            raise ValueError("endpoint does not match reference + axis * budget")
        travel = dot(sub(final, start), axis)
        residual = sub(sub(final, start), tuple(a * travel for a in axis))
        if travel < -tolerance or travel > budget + tolerance or math.sqrt(dot(residual, residual)) > tolerance:
            raise ValueError("final vertex lies outside its queried movement segment")
    witness = vector(trace["support_point"])
    planes = support_planes(starts + ends)
    normal, offset = max(planes, key=lambda plane: dot(plane[0], witness) - plane[1])
    excess = dot(normal, witness) - offset
    excluded = excess > clearance + tolerance
    return {"status": "excluded_from_clearance_sweep" if excluded else "not_excluded",
            "source": trace.get("source"), "face": trace.get("face"),
            "clearance": clearance, "tolerance": tolerance,
            "maximum_plane_excess": excess, "separating_normal": normal,
            "support_offset": offset, "plane_count": len(planes),
            "interpretation": (
                "arm witness lies outside the clearance sweep; recompute full clipped support before changing solver"
                if excluded and trace.get("source") == "arms" else
                "outside geometric sweep; inner-cloth outermost containment may still require this witness"
                if excluded and trace.get("source") == "inner" else
                "outside geometric sweep; support semantics are unknown"
                if excluded else
                "no separating proof; this does not establish actual contact")}


def traces(value, location="$", depth=0):
    if depth > 64:
        raise ValueError("JSON nesting exceeds audit limit")
    if isinstance(value, dict):
        if "support_point" in value and "face" in value:
            yield location, value
        else:
            for key, child in value.items():
                yield from traces(child, f"{location}.{key}", depth + 1)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from traces(child, f"{location}[{index}]", depth + 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="+", type=Path)
    parser.add_argument("--clearance", type=float, default=0.032)
    args = parser.parse_args()
    for path in args.files:
        value = json.loads(path.read_text())
        count = 0
        for location, trace in traces(value):
            count += 1
            try:
                result = audit(trace, args.clearance)
            except (ValueError, KeyError, TypeError) as error:
                result = {"status": "invalid_evidence", "reason": str(error)}
            print(json.dumps({"file": str(path), "location": location, **result}))
        if count == 0:
            print(json.dumps({"file": str(path), "status": "no_face_traces"}))


if __name__ == "__main__":
    main()
