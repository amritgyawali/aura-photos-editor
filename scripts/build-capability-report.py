"""Build the evidence-backed report after the desktop audit scripts complete."""
import json
import re
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.work-checks/full-audit'
def read(name):return json.loads((OUT/name).read_text(encoding='utf-8'))
runtime=read('runtime-results.json')
followup=read('followup-results.json')
isolated=read('isolated-results.json')
gui=read('gui/results.json')
export=read('gui/export-results.json')
restart=read('restart-results.json')
native=read('native/results.json')
manifest=read('input-manifest.json')
catalog=json.loads((ROOT/'docs/photo-editor-feature-roadmap.json').read_text())

tested={1,7,11,12,13,15,16,18,19,20,21,22,23,25,26,27,31,32,37,41,42,43,44,49,51,52,54,55,57,58,59,66,76,81,82,86,89,90,92,94,99}
partial={2,3,4,5,6,8,10,14,28,29,33,34,35,36,45,46,47,48,50,53,56,60,63,64,74,75,83,85,88,91,98}
defects={84,93,97}
notes={
1:'File/folder IPC import and desktop collection selection tested. Windows file picker was not automated. Same-directory stems are grouped as one photo.',
2:'81 RAW/export integration tests include synthetic RAW fixtures. No real camera RAW file was qualified; proprietary compression support varies.',
3:'EXIF orientation 6 swapped dimensions correctly. Real MakerNotes and all orientation values were not tested.',
4:'Catalog and UI foundations; not exercised through a complete tagging workflow in this audit.',
5:'Metadata storage exists; comprehensive search and keyword editing not established here.',
6:'New collections and navigation tested; smart/rule-based albums not established.',
7:'Repeated import reported skipped existing files. Same-stem derivative grouping can hide an individually editable PNG behind a TIFF.',
8:'Real YuNet face detection in auto retouch; this is not identity recognition. Older people-pipeline detector is a placeholder.',
10:'Cached previews exist; editing with an original disconnected was not tested.',
14:'Highlight slider changes pixels. It cannot recover detail already clipped out of a JPEG.',
18:'Histogram visible in the live desktop; unit test passed. No instrumented colorimetry validation.',
19:'Live clipping overlay toggled. Preview clipping is not sensor-level RAW clipping.',
28:'Calibration sliders work; measured camera matching profiles remain incomplete.',
29:'Output/profile infrastructure exists. No calibrated monitor, wide-gamut roundtrip or print proof certification.',
33:'Small-angle rotation tested; full rotate/mirror workflow not fully exercised.',
34:'Perspective implementation exists; not exercised through the desktop in this audit.',
35:'Switch saved but changed no pixels on the sample without a calibrated lens profile.',
36:'Switch saved but changed no pixels on the sample; no measured chromatic-aberration chart tested.',
41:'Actual mouse brush, erase, stroke undo and Enter-to-apply tested in native retouch.',
42:'Native retouch gradient implementation and focused tests; not general adjustment-layer gradients.',
43:'Feathered retouch ellipse; not a full layer-based radial-adjustment system.',
44:'Native retouch luminance gate; covered by renderer/UI tests, not a new real-photo GUI range-mask session.',
45:'Sampled skin/color-affinity selection; not unrestricted semantic/color selection.',
46:'Semantic segmentation model is untrained. No reliable automatic subject/background masking claim.',
47:'Semantic segmentation model is untrained. No reliable automatic sky mask claim.',
48:'Automatic face boxes/landmarks and cheek/forehead sampled-color masks work. No learned skin/hair/eye segmentation.',
49:'Feather/edge-protection processors have focused checks; no perfect hair-edge guarantee.',
50:'Brush add/erase, inversion and range intersections exist. Arbitrary layer-mask Boolean workflows are incomplete.',
51:'Heal, patch heal and local spot cleanup change pixels. Spot heuristics cannot decide whether a mark is permanent.',
52:'Sampled clone renders and passes exact isolated Undo/Redo.',
53:'Small-area heal/patch tools work; general semantic/generative object removal is not production-ready.',
54:'Manual red-eye tool renders; no diverse flash-red-eye perceptual benchmark.',
55:'Sampled smoothing and auto face-guided texture pass work. Not commercial quality parity.',
56:'Frequency-based tone/detail processor works; separate editable high/low pixel layers are unavailable.',
57:'Manual dodge/burn and automatic sampled-skin light balancing work.',
58:'Manual eye/teeth processors work; no automatic semantic sclera/teeth targeting.',
59:'Sampled skin tone uniformity works; similar-colored non-skin pixels can match.',
60:'No production depth map/background blur workflow established.',
63:'Retouch selections are per operation; general pixel-layer masks unavailable.',
64:'Retouch strength exists; general layer blend modes unavailable.',
66:'Native retouch operation stack exists; not a universal smart-filter/layer stack.',
74:'No multi-frame noise-reduction workflow established.',
75:'Classical noise sliders work. Learned denoise and face-recovery heads remain untrained.',
76:'Sharpening, texture, clarity and dehaze changed sample pixels; quality is not benchmarked against competitors.',
81:'21 profiles listed; first profile at 65% applied repeatably. Not all 21 profiles visually graded.',
82:'Selected tone settings synced to one target with manual protection; full collection stress sync not tested.',
83:'Reference-style analysis/matching implementation and UI tests exist; no new end-to-end reference set audit.',
84:'Normal isolated Undo/Redo passed for all 24 tools and keyboard workflow. Reset photo fails on retouch-only edits.',
85:'Named snapshot saved through keyboard. Restore and virtual-copy workflows not fully exercised.',
86:'Live compare divider, zoom, pan, fit and before/after tested.',
88:'Multi-photo IPC processing and export tested. Thousands-of-images queue/cancel/resume not qualified.',
91:'JPEG/PNG and true 16-bit TIFF export verified. Ordinary RGB/gray TIFF input fails; PNG alpha/16-bit precision not retained.',
92:'256-pixel long-edge resize verified; output sharpening exercised by export integration tests.',
93:'Original-name template produced image/image_2/image_3. Privacy policy checks are structural, not forensic real-camera metadata qualification.',
94:'Text/logo watermark UI and native compositing exist; synthetic RGBA watermark plus 256-pixel export verified.',
97:'Requested export sidecars were absent. Export field supplies recipe_json=None; XMP serialization elsewhere does not prove working interchange.',
98:'CPU renderer used. GPU shaders/ports do not constitute an active GPU rendering backend.',
99:'Preview/cache implementation and earlier cache tests; final edited recipe/pixels persisted through forced restart.',
}
features=[]
for f in catalog['features']:
    i=f['id']
    status='Tested with limits' if i in tested else 'Partial / limited' if i in partial else 'Known failure' if i in defects else 'Unavailable / not established'
    features.append(dict(id=i,name=f['name'],category=f['category'],status=status,evidence=f['evidence'],
                         note=notes.get(i,'Current source/documentation does not establish this complete desktop workflow.' if status.startswith('Unavailable') else 'Sample execution or focused checks passed; not universal format or visual-quality proof.')))

findings=[
 ('High','Reset photo leaves native retouch','Fresh isolated photo: add Dodge, Reset photo, render. Reset pixels equal Dodge pixels, not original. Removing the extra recipe key is missed because changed_paths iterates only paths in the target recipe. Fix deleted-path detection and add an application-level regression.','crates/aura-recipe/src/history.rs:326'),
 ('High','Valid TIFF input rejected as damaged','Fresh single-file collections reject Pillow-valid RGB8 and grayscale16 TIFF with AURA-RAW-2002. TIFF export works. Add ordinary TIFF decoding and accurate unsupported-format reporting.','crates/aura-raw/src/meta.rs'),
 ('High','Requested sidecars not written','All three-format exports requested sidecar=true; each reported sidecars=0. ExportField initializes recipe_json and original_path to None. Connect persisted recipes/originals and verify sidecar contents.','crates/aura-app/src/delivery_commands.rs:172'),
 ('Medium','Original filenames lost on export','{original} produced image, image_2, image_3. ExportField queries original_name/camera_model while actual file names live in photo_file; errors/fallbacks lose naming data. Use the catalog primary-file relationship.','crates/aura-app/src/delivery_commands.rs:178'),
 ('Medium','Manual edits labelled automatic','The saved GUI Exposure, Orange, Crop and Native retouch history entries carry source=ai after an automatic pass. Manual protection still works. Fix provenance when committing user edits.','crates/aura-app/src/native_retouch.rs'),
 ('Medium','Portrait decode timeout under load','One 1200×1800 JPEG initially failed AURA-RAW-2004 during concurrent tests. Fresh isolated retry and face retouch passed. The error says retryable=false while asking the user to retry. Root cause not established.','crates/aura-raw/src/timeout.rs'),
 ('Medium','Window close did not exit promptly','CloseMainWindow returned true, but the audited process was still alive after 20 seconds. Only that process was stopped for restart. Saved recipe and pixels recovered exactly. Shutdown cause not established.','restart-results.json'),
 ('Build','Clean native test run blocked','cargo test -p aura-raw -p aura-export --lib crashed rustc compiling aura-catalog (0xc0000409), including a retry with incremental disabled. Focused integrations compiled against the existing desktop dependency graph and passed. This is not a green full-workspace gate.','full-audit-native-tests-noincremental.log'),
 ('Build','Local dependency corruption and advisories','Initial UI suite could not load mime-db/db.json. Preserved the error, restored with npm ci, then all 553 tests and production build passed. npm audit reports 5 development-tool findings: 3 moderate, 1 high, 1 critical. No exploitability assessment or dependency upgrade was made.','full-audit-npm-security.json'),
]

evidence=dict(date='2026-09-30',commit='3a546d1127ed01ee094601eab084df6ccfcc65d2',
 executableSha256='6b230674c7fee706db1e153c6d39aa73be8c876b6dd2344c9298ca70b2075e10',
 verdict='Useful JPEG/PNG photo editor and portrait retoucher; not an all-purpose professional editor for every photograph.',
 platform='Windows 11 Home 10.0.26200, about 7.8 GiB RAM, CPU debug build; unrelated background workloads left running',
 unitChecks=dict(uiPassed=553,uiFiles=57,focusedNativePassed=110,fullNativeGate='blocked by compiler crash'),
 inputs=manifest,features=features,findings=[dict(severity=s,title=t,detail=d,evidence=e) for s,t,d,e in findings],
 runtime=runtime,followup=followup,isolated=isolated,gui=gui,export=export,restart=restart,native=native)
archive=ROOT/'docs/audits'
archive.mkdir(parents=True,exist_ok=True)
(archive/'2026-09-30-capability-results.json').write_text(json.dumps(evidence,indent=2),encoding='utf-8')

lines=['# AURA capability audit — 30 September 2026',
 '**Verdict: AURA cannot yet edit every kind of photograph or replace a complete professional editing suite.** It can edit supported JPEG/PNG photos, automatically detect suitable portrait faces, perform restrained skin retouch, accept manual refinements, and export verified results. Significant format, history, export-metadata and advanced-feature gaps remain.',
 'This is a broad audit of the current build, not proof of every possible photo, camera, control combination or commercial quality equivalence. No Retouch4me/SkinFiner installation was available for an A/B comparison.',
 '## Build and method',
 '- Source: `3a546d1127ed01ee094601eab084df6ccfcc65d2`; executable SHA-256: `'+evidence['executableSha256']+'`.',
 '- Windows 11 Home 10.0.26200; approximately 7.8 GiB RAM. Existing CPU debug desktop, WebView2 on CDP port 9223. Background work was preserved. Timings are not release benchmarks.',
 '- 25 input files: eight distinct real Pexels photographs (five portraits, landscape, veterinary scene, sneaker product), plus 17 derived/format/corruption fixtures. The second RGB PNG is a byte-identical alias added to separate the same-stem TIFF/PNG grouping; do not count it as an independent photo. Same-stem companions and content deduplication explain why input file count differs from catalog-photo count.',
 '- Actual desktop mouse and keyboard controls were exercised through Playwright/WebView2. Bulk imports, pixel comparisons, isolated format and processor probes used native IPC. This did not automate the Windows file picker, a physical pen/tablet, every PC application, or every keyboard key.',
 '- Originals were opened read-only; edits live in test collections and exports. All 25 audit input SHA-256 hashes remained unchanged. Public photographs were downloaded for local testing; no personal photographs were uploaded.',
 '## Verification totals',
 '| Check | Result |\n|---|---|\n| UI suite | 553 passed across 57 files after lockfile dependency reinstall |\n| TypeScript and Vite production build | Passed |\n| Focused native integrations | 110 passed: retouch 29, RAW/color/container/PNG/tiers 68, export/watermark 13 |\n| All 24 native retouch processors | Pixel changes and exact isolated Undo/Redo passed |\n| Develop parameter probes | 32 executed; 30 changed pixels; lens distortion and CA switches had no effect on the unprofiled sample |\n| JPEG/PNG/TIFF export | 3 files each, 9 written/read-back verified; 3 PNGs exactly matched full renderer; TIFF tags report 16 bits/channel |\n| Resized watermark export | 1 verified PNG at 256 × 170 |\n| Real GUI portrait export | 1 verified JPEG at 640 × 800 |\n| Restart | Final recipe hash and pixel hash unchanged after forced restart |\n| Full native/workspace gate | Not passed: rustc crashed during catalog compilation |',
 'The focused native tests were compiled from current test sources against the existing desktop rlibs. Initial fallback harness time-crate mismatches were corrected by resolving the exact dependency fingerprint. Synthetic codec roundtrips do not qualify real camera files.',
 '## What works in the tested scope',
 '- Import and browse JPEG/PNG collections; skip repeat imports; render actual source pixels; preserve originals.',
 '- Automatic light correction and YuNet face detection. All five real portraits eventually received three editable operations each. The composed two-person fixture detected both faces. Landscape, sneaker and veterinary scenes produced no suitable human face and skipped portrait retouch.',
 '- Sampled skin texture smoothing, tone uniformity and local dodge/burn. Automatic masks are landmark-guided regions with sampled-color affinity and eye/mouth exclusions, not learned full skin segmentation.',
 '- Exposure, temperature/tint, contrast, tonal sliders, RGB/point/parametric curves, HSL, grading, B&W, texture/clarity/dehaze, sharpening/noise sliders, grain, vignette, calibration adjustment, crop and rotation.',
 '- Native manual heal/patch/clone, spot cleanup, frequency-based tone/detail processing, dodge/burn, skin color, eye/teeth adjustments, fine-line/shine reduction, fabric/backdrop and glare softening. Their names do not imply independent learned AI models.',
 '- Painted retouch selections, erase, feathering, gradients/ranges/inversion in the native implementation, operation history, comparison, zoom/pan, 21 listed adaptive profiles, selected settings sync and snapshot saving.',
 '## What is limited or unavailable',
 '- **Formats:** HEIC/HEIF unsupported; WebP/BMP/GIF filtered by importer. Ordinary RGB/gray TIFF input failed despite TIFF output working. PNG transparency is composited onto white; 16-bit PNG input is reduced to 8-bit. CMYK JPEG opened, but no CMYK/ICC color-fidelity certification was performed.',
 '- **RAW:** synthetic codec coverage only in this audit. CR3/CRX, compressed RAF, RW2 and other proprietary variants have documented restrictions/preview fallbacks. No promise of full-resolution sensor editing for every camera.',
 '- **Retouch intelligence:** no reliable semantic skin/hair/teeth/sky/background models in the older placeholder pipelines; no identity recognition established; no automatic perfect removal of blemishes/permanent marks, reconstruction of hidden glare detail, or guaranteed flattering edit.',
 '- **General editor:** no established full pixel-layer/Smart Object/text/vector document system, layered PSD interchange, liquify/content-aware scaling, panorama/HDR/focus merge, generative fill/expand, sky replacement, AI super-resolution, print layouts or soft proofing.',
 '- **Performance/color:** CPU rendering; no active GPU backend. High-resolution wedding throughput, calibrated monitor/print accuracy, HDR display/export, multi-monitor DPI and 45–100 MP memory behavior remain unqualified.',
 '## Confirmed failures and priorities',
 '| Priority | Finding | Evidence and next action |\n|---|---|---|']
for severity,title,detail,source in findings:
    lines.append(f'| {severity} | {title} | {detail} Source: `{source}`. |')
lines += ['## Step-by-step edit I performed',
 'Source: [Pexels portrait 1239291](https://www.pexels.com/photo/1239291/). Separate collection: `'+gui['collectionName']+'`.',
 '1. Imported the source into an isolated collection through IPC, then selected it using the desktop collection and photo controls. Saved the initial full render.\n2. Clicked **Auto enhance photo**: one face, three editable skin/light operations.\n3. Typed **+0.35 EV** and **6000 K** using Ctrl+A and Enter.\n4. Added a tone-curve point with the mouse; set orange saturation to **−12**.\n5. Clicked **4:5** crop and saved **Audit natural portrait** as a named snapshot with Enter.\n6. Moved the before/after divider with Home/ArrowRight and enabled clipping warnings.\n7. Opened Retouch, selected Dodge, pressed **B**, increased brush size with **]**, painted with a mouse drag, pressed **E**, erased a dab, undid the stroke, and applied with **Enter**.\n8. Tested **+**, arrow-key pan, **H**, **0**, split comparison, and **Ctrl+Z / Ctrl+Shift+Z** on the saved operation.\n9. Entered an output path using keyboard controls, previewed export names, clicked **Export**, and verified the UI reported one file written and checked.\n10. Restarted AURA and verified identical final recipe/pixel hashes.',
 'The first exposure test used exact floating-point equality and falsely failed at `0.3499999940395355`. A dedicated mouse/keyboard repeat with 1e-5 tolerance passed; no application fix was needed. Test code now uses tolerance. Initial raw logs remain preserved.',
 'The final edit is a workflow demonstration with a deliberately visible brush correction, not a best-quality retouch benchmark. The supplied portraits were already professionally lit; they do not prove acne/freckle discrimination, severe repair or fairness across a representative population.',
 '## Reproduction details and ambiguous results resolved',
 '- Start the existing desktop with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`. Run `scripts/prepare-audit-fixtures.py`, `test-capability-audit.py`, `test-audit-gui.py`, `test-audit-followup.py`, `test-audit-isolated.py`, then `test-audit-export-ui.py`. Tests create separate collections and write under `.work-checks/full-audit/`. Preserve results before a new run; `--resume` retains completed capability cases.',
 '- The initial same-stem `test-rgb.png`/`test-rgb.tif` pair selected TIFF as primary. A duplicate PNG alias later altered catalog association, making one TIFF-labelled retry appear to pass. **Fresh single-file TIFF collections reproduced the rejection**; the ambiguous retry is not treated as TIFF support. The fixture generator now gives the opaque PNG a distinct stem.',
 '- Initial 23 tool-history failures followed a retouch-only Reset that did not clear the prior effect. A second pass using explicit stack clearing showed all 24 processors and ordinary Undo/Redo working. A fresh-photo reset reproduction confirms the independent Reset defect.',
 '- Extreme exposure 99999 was accepted but correctly clamped to +5 EV; this is not an out-of-range execution bug.',
 '- Corrupt JPEG rejection, unchanged flat black/one-pixel images and no face on a 90-degree EXIF-rotated face are bounded/expected behaviors, not counted as universal editing failures.',
 '- Scope excludes clean full-workspace CI, real-camera RAW certification, competitor A/B testing, accessibility with a screen reader, cloud/provider delivery, paid APIs, huge catalogs, power-loss durability and every tool combination. Existing tests cover parts of these modules; that is not equivalent to end-to-end qualification.',
 '## All 100 requested feature families',
 'Statuses are scope labels, not a percentage-complete score. “Tested with limits” means the stated sample or focused check passed. “Partial / limited” can include working components. “Unavailable / not established” means the complete workflow is not supported by the inspected implementation/documentation. The original roadmap statuses are historical; this table is the current audit.',
 '| # | Feature | Current assessment | Limits / evidence |\n|---|---|---|---|']
for f in features:lines.append(f'| {f["id"]} | {f["name"]} | {f["status"]} | {f["note"]} `{f["evidence"]}` |')
lines += ['## All 24 native retouch processors',
 '| Processor | Pixel effect | Exact isolated Undo/Redo |\n|---|---|---|']
for c in followup['cases']:
    if c['name'].startswith('isolated-tool:'):
        r=c['result'];lines.append(f'| {c["name"].split(":")[1]} | {r["changed"]} | {r["exactUndo"] and r["exactRedo"]} |')
lines += ['## Files and evidence',
 '- [Machine-readable report](audits/2026-09-30-capability-results.json) includes per-case results, input hashes, model reports, GUI history and source links.\n- [Interactive local report](../.work-checks/full-audit/report.html) contains before/after images and searchable feature/issue tables.\n- [Edited PNG](../.work-checks/full-audit/gui/final.png) and [exported JPEG](../.work-checks/full-audit/'+export['files'][0]['file'].replace('\\','/')+').\n- Local raw logs and screenshots: `.work-checks/full-audit*`; input sources are individually attributed in `input-manifest.json`. Downloaded photographs and large screenshots are local evidence, not committed source assets.',
 '## Recommended completion order',
 '1. Fix retouch reset and manual provenance; verify reset/snapshot/history transitions with removed recipe fields.\n2. Fix real catalog-to-export filename/recipe wiring; test exported sidecars and roundtrip interchange.\n3. Implement ordinary TIFF input and accurately disclose unsupported/precision-losing formats.\n4. Resolve shutdown/decode stalls and the local native compiler failure; run clean full CI and representative high-resolution workloads.\n5. Train/validate semantic models and integrate one reliable face pipeline across the product; benchmark diverse real images with human review.\n6. Treat layers, merging, generative tools and other absent workflows as separate implementation projects.',
 'No application behavior was changed as part of this audit. Dependency restoration, test harnesses, evidence and reporting are the changes. The failures above remain open.']
markdown='\n\n'.join(lines)+'\n'
# Table rows must be adjacent for CommonMark/GitHub rendering.
markdown=re.sub(r'(?m)(^\|[^\n]*\|)\n\n(?=\|)',r'\1\n',markdown)
(ROOT/'docs/software-capability-audit.md').write_text(markdown,encoding='utf-8')

# Offline report: local images plus embedded structured results, no CDN or tracking.
data=json.dumps({'features':features,'findings':evidence['findings']}).replace('</','<\\/')
page='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA capability audit</title>
<style>body{font:16px/1.55 system-ui;max-width:1280px;margin:32px auto;padding:0 22px;color:#20232b;background:#fafafa}h1{font-size:30px}h2{font-size:22px;margin-top:36px}p{max-width:950px}table{border-collapse:collapse;width:100%}td,th{text-align:left;vertical-align:top;border-bottom:1px solid #d8dbe1;padding:10px}th{background:#eef0f3}input,select{padding:9px;font:inherit;margin:4px 8px 12px 0}summary{cursor:pointer;font-weight:600}figure{margin:0}img{max-width:100%;max-height:620px;object-fit:contain}.photos{display:grid;grid-template-columns:1fr 1fr;gap:20px}small{color:#555}tr[hidden]{display:none}@media(max-width:750px){.photos{grid-template-columns:1fr}}</style>
<h1>AURA: what works, what still fails</h1><p><strong>Not ready to edit every kind of photo.</strong> Supported JPEG/PNG editing and portrait retouch work; format coverage, retouch reset and export metadata need fixes. This is a build audit, not a competitor quality certification.</p>
<p>30 September 2026 · source 3a546d1 · 553 UI tests and 110 focused native tests passed · 24 retouch tools exercised · full native build gate blocked.</p>
<h2>The photograph edited through the desktop</h2><div class="photos"><figure><img src="gui/before.png" alt="Original portrait before editing"><figcaption>Before · original 1200 × 800</figcaption></figure><figure><img src="gui/final.png" alt="Portrait after actual AURA desktop editing"><figcaption>After · 640 × 800 crop, automatic skin steps and manual tone/color/brush edits</figcaption></figure></div>
<p><a href="gui/13-export-complete.png">Export screenshot</a> · <a href="gui/14-after-restart.png">Reopened desktop</a> · <a href="../../docs/software-capability-audit.md">Full written report</a> · <a href="https://www.pexels.com/photo/1239291/">Photo source</a></p>
<h2>Open findings</h2><div id="issues"></div><h2>100-feature checklist</h2><p><label>Search <input id="search" type="search" placeholder="Try RAW, layers, skin…"></label><label>Assessment <select id="filter"><option>All</option><option>Tested with limits</option><option>Partial / limited</option><option>Known failure</option><option>Unavailable / not established</option></select></label><output id="count"></output></p><table><thead><tr><th>#</th><th>Feature</th><th>Assessment</th><th>Scope and evidence</th></tr></thead><tbody id="rows"></tbody></table>
<script>const data=DATA;const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));document.querySelector('#issues').innerHTML=data.findings.map(f=>'<details><summary>'+esc(f.severity+' · '+f.title)+'</summary><p>'+esc(f.detail)+'</p><small>'+esc(f.evidence)+'</small></details>').join('');function draw(){const q=document.querySelector('#search').value.toLowerCase(),f=document.querySelector('#filter').value;const rows=data.features.filter(r=>(f==='All'||r.status===f)&&JSON.stringify(r).toLowerCase().includes(q));document.querySelector('#count').textContent=rows.length+' of 100 features';document.querySelector('#rows').innerHTML=rows.map(r=>'<tr><td>'+r.id+'</td><td>'+esc(r.name)+'</td><td>'+esc(r.status)+'</td><td>'+esc(r.note)+'<br><small>'+esc(r.evidence)+'</small></td></tr>').join('')}document.querySelector('#search').oninput=draw;document.querySelector('#filter').onchange=draw;draw();</script></html>'''.replace('DATA',data)
(OUT/'report.html').write_text(page,encoding='utf-8')

canvas='''import { useCanvasState, useHostTheme } from 'cursor/canvas';
const data = DATA;
export default function AuraAudit() {
 const theme = useHostTheme();
 const [query, setQuery] = useCanvasState('search', '');
 const [status, setStatus] = useCanvasState('status', 'All');
 const rows=data.features.filter(row=>(status==='All'||row.status===status)&&JSON.stringify(row).toLowerCase().includes(query.toLowerCase()));
 return <main style={{color:theme.text.primary,background:theme.bg.editor,padding:24,fontFamily:'system-ui',lineHeight:1.5}}>
 <h1 style={{fontSize:24}}>AURA capability audit</h1><p><strong>Useful photo editing. Incomplete professional coverage.</strong></p>
 <p style={{color:theme.text.secondary}}>30 September 2026 · 3a546d1 · 553 UI / 110 focused native tests passed. Full native build gate blocked.</p>
 <p>24 retouch tools changed pixels and passed isolated Undo/Redo. Reset photo can leave retouch behind. TIFF input, sidecars and original-name exports need fixes. No claim of Retouch4me or SkinFiner quality parity.</p>
 <h2 style={{fontSize:20}}>Findings</h2>{data.findings.map(f=><details key={f.title} style={{borderBottom:`1px solid ${theme.stroke.primary}`,padding:'8px 0'}}><summary>{f.severity} · {f.title}</summary><p>{f.detail}</p><small style={{color:theme.text.secondary}}>{f.evidence}</small></details>)}
 <h2 style={{fontSize:20}}>100-feature checklist</h2><label>Search <input value={query} onChange={e=>setQuery(e.target.value)} style={{background:theme.bg.elevated,color:theme.text.primary,border:`1px solid ${theme.stroke.primary}`,padding:8}} /></label>{' '}
 <label>Assessment <select value={status} onChange={e=>setStatus(e.target.value)} style={{background:theme.bg.elevated,color:theme.text.primary,padding:8}}>{['All','Tested with limits','Partial / limited','Known failure','Unavailable / not established'].map(s=><option key={s}>{s}</option>)}</select></label><p style={{color:theme.text.secondary}}>{rows.length} of 100 features. Status labels describe bounded evidence, not a completion score.</p>
 <table style={{width:'100%',borderCollapse:'collapse'}}><thead><tr>{['#','Feature','Assessment','Limits'].map(s=><th key={s} style={{textAlign:'left',padding:8,borderBottom:`1px solid ${theme.stroke.primary}`}}>{s}</th>)}</tr></thead><tbody>{rows.map(row=><tr key={row.id}>{[row.id,row.name,row.status,row.note].map((v,i)=><td key={i} style={{verticalAlign:'top',padding:8,borderBottom:`1px solid ${theme.stroke.secondary}`}}>{v}</td>)}</tr>)}</tbody></table>
 </main>;
}
'''.replace('DATA',data)
(OUT/'audit.canvas.tsx').write_text(canvas,encoding='utf-8')
print('Wrote full report, archived JSON, offline HTML and canvas source.')
