"""Geometry checks for native-rendered retouch evidence; no image processing."""
import math
import numpy as np


def is_compact_spot(edit):
    return edit['tool'] in ('patch_heal','heal') or (
        edit['tool']=='skin_uniformity' and '-spot-deep-' in edit.get('id','')
        and max(edit['region'][2:]) <= .1)


def only_local_spot_changes(points, edits, width, height):
    patches = [e for e in edits if is_compact_spot(e) and e.get('enabled', True)]
    return all(any(math.hypot(((x+.5)/width-e['region'][0])/e['region'][2],
                 ((y+.5)/height-e['region'][1])/e['region'][3]) < 1 for e in patches)
               for x, y in points)


def coarse_region_change(before, after, bounds, eye_distance):
    """Measure coarse color/luminance change; never produce or edit photo pixels."""
    h, w, _ = before.shape
    l, t, r, b = bounds
    x0, y0, x1, y1 = int(l*w), int(t*h), int(r*w), int(b*h)
    # Never round below the requested anatomical scale at export resolution.
    radius = max(2, math.ceil(eye_distance*.04))
    a0, a1 = max(0, x0-2*radius), max(0, y0-2*radius)
    z0, z1 = min(w, x1+2*radius), min(h, y1+2*radius)
    baseline = before[a1:z1, a0:z0].astype(np.float64)/255
    edited = after[a1:z1, a0:z0].astype(np.float64)/255

    def blur(values):
        padded = np.pad(values, radius, mode='edge')
        integral = np.pad(padded, ((1, 0), (1, 0))).cumsum(0).cumsum(1)
        size = 2*radius+1
        return (integral[size:, size:]-integral[:-size, size:]
                -integral[size:, :-size]+integral[:-size, :-size])/(size*size)

    region = (slice(y0-a1, y1-a1), slice(x0-a0, x1-a0))
    # This gate measures changes within its named structure window. Repairs in
    # neighboring skin are reviewed separately and must not contaminate this gate.
    absolute = np.max(np.abs(edited-baseline), axis=2)
    scoped = np.zeros_like(absolute)
    scoped[region] = absolute[region]
    # Absolute change cannot cancel an inverted contour or an opposing color shift.
    change = blur(blur(scoped))
    light = blur(blur(baseline @ np.array([.2126, .7152, .0722])))
    ratio = change[region]/np.maximum(light[region], .02)
    return {'max_relative_coarse_change': float(ratio.max()),
            'mean_relative_coarse_change': float(ratio.mean()), 'analysis_radius': radius}


def only_local_feather_changes(points, edits, width, height):
    """Every changed coordinate lies in a compact repair's feather, outside its core."""
    patches = [e for e in edits if is_compact_spot(e) and e.get('enabled', True)]
    for x, y in points:
        covered = False
        for edit in patches:
            cx, cy, rx, ry = edit['region']
            distance = math.hypot(((x+.5)/width-cx)/rx, ((y+.5)/height-cy)/ry)
            if distance <= 1-edit['feather']:
                return False
            if distance < 1:
                covered = True
        if not covered:
            return False
    return True
