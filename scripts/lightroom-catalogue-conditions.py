"""Conditions and per-photo adaptation, from the JSON lightroom-catalogue-study.py writes.

    python scripts/lightroom-catalogue-conditions.py out.json
"""
import json, statistics, sys
from collections import Counter, defaultdict
data = [d for d in json.load(open(sys.argv[1], encoding='utf-8')) if d['edited'] and d['format'] == 'RAW']
# A photo is hand-developed when anything beyond the import defaults is set.
TONE = ['Exposure2012','Contrast2012','Highlights2012','Shadows2012','Whites2012','Blacks2012','Texture','Clarity2012','Dehaze','Vibrance']
dev = [d for d in data if any(d['s'].get(k) not in (None, 0, 0.0) for k in TONE) or d['wb_mode'] == 'Custom']
print('RAW developed', len(data), 'hand-developed', len(dev))
def hour(d):
    try: return int(d['capture'][11:13])
    except Exception: return None
def cond(d):
    h = hour(d); iso = d['iso'] or 0; flash = d['flash'] == 1
    when = 'night' if h is not None and (h >= 18 or h < 6) else 'day'
    light = 'flash' if flash else ('ambient-bright' if iso <= 400 else 'ambient-dim' if iso <= 1600 else 'ambient-dark')
    return f'{when}/{light}'
def med(ds, k):
    v = [d['s'][k] for d in ds if isinstance(d['s'].get(k), (int, float))]
    return statistics.median(v) if v else float('nan')
def spread(ds, k):
    v = [d['s'][k] for d in ds if isinstance(d['s'].get(k), (int, float))]
    return (sorted(v)[int(len(v)*.9)] - sorted(v)[int(len(v)*.1)]) if len(v) > 4 else float('nan')
groups = defaultdict(list)
for d in dev: groups[cond(d)].append(d)
print(f"\n{'condition':22} {'n':>5} {'temp':>6} {'tint':>5} {'exp':>6} {'contr':>5} {'hi':>5} {'sh':>5} {'wh':>5} {'bl':>5} {'tex':>4} {'clar':>5} {'dhz':>4} {'vib':>4} {'custWB%':>7}")
for g, ds in sorted(groups.items()):
    cust = 100*sum(d['wb_mode']=='Custom' for d in ds)/len(ds)
    print(f"{g:22} {len(ds):5} {med(ds,'Temperature'):6.0f} {med(ds,'Tint'):+5.0f} {med(ds,'Exposure2012'):+6.2f} {med(ds,'Contrast2012'):+5.0f} {med(ds,'Highlights2012'):+5.0f} {med(ds,'Shadows2012'):+5.0f} {med(ds,'Whites2012'):+5.0f} {med(ds,'Blacks2012'):+5.0f} {med(ds,'Texture'):+4.0f} {med(ds,'Clarity2012'):+5.0f} {med(ds,'Dehaze'):+4.0f} {med(ds,'Vibrance'):+4.0f} {cust:7.0f}")
# Per-photo adaptation: within a shoot and a condition, how far each slider varies photo to photo.
print('\nPer-photo variation inside one shoot and condition (p90-p10), median across groups:')
keys = ['Temperature','Tint','Exposure2012','Highlights2012','Shadows2012','Whites2012','Blacks2012','Contrast2012','Vibrance','CropAngle']
sub = defaultdict(list)
for d in dev: sub[(d['shoot'], cond(d))].append(d)
for k in keys:
    v = [spread(ds, k) for ds in sub.values() if len(ds) >= 20]
    v = [x for x in v if x == x]
    print(f'  {k:16} {statistics.median(v) if v else float("nan"):8.2f}   (groups {len(v)})')
print('\nWhite balance mode by condition:')
for g, ds in sorted(groups.items()):
    print(f'  {g:22}', Counter(d['wb_mode'] for d in ds).most_common())
