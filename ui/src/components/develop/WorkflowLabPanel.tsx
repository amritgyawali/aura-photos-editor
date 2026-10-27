import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, develop } from '../../ipc/client';
import { workflowLab, type WorkflowAction, type WorkflowRequest, type WorkflowSnapshot } from '../../ipc/workflowLab';
import type { ImageRowLite, RecipeDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';

export interface WorkflowLabPanelProps {
  projectId: string;
  photoId: string;
  photoIds?: readonly string[];
  recipe: RecipeDto | null;
  disabled: boolean;
  onEdited?: () => Promise<void> | void;
}
const SECTIONS = ['tone', 'white_balance', 'curves', 'colour', 'detail', 'effects', 'geometry'];
const STATES = ['', 'unreviewed', 'in_review', 'approved', 'changed_since_review', 'changes_requested'];
function Rows({ rows }: { rows: WorkflowSnapshot['rows'] }): JSX.Element {
  return <table><thead><tr><th>Photograph</th><th>Changes</th><th>Conflicts</th></tr></thead><tbody>{rows.map(row => <tr key={row.photoId}>
    <td>{row.photoId.slice(0, 12)}</td><td>{row.changed.join(', ') || 'No changes'}</td><td>{row.conflicts.join(', ') || 'None'}</td>
  </tr>)}</tbody></table>;
}
/** Durable collection workflows. Requests are explicit; revision changes invalidate loaded work. */
export function WorkflowLabPanel({ projectId, photoId, photoIds = [], recipe, disabled, onEdited }: WorkflowLabPanelProps): JSX.Element {
  const initial = [...new Set([photoId, ...photoIds])].slice(0, 32);
  const [cohort, setCohort] = useState(initial.join('\n'));
  const [snapshot, setSnapshot] = useState<WorkflowSnapshot | null>(null);
  const [busy, setBusy] = useState(false); const [message, setMessage] = useState(''); const [error, setError] = useState('');
  const [name, setName] = useState('Review baseline'); const [checkpointId, setCheckpointId] = useState('');
  const [sections, setSections] = useState(['tone']); const [source, setSource] = useState(photoId);
  const [criteria, setCriteria] = useState('Colour\nSkin detail\nComposition'); const [criterion, setCriterion] = useState('');
  const [reviewer, setReviewer] = useState(''); const [dueDate, setDueDate] = useState('');
  const [text, setText] = useState(''); const [issueId, setIssueId] = useState(''); const [dependency, setDependency] = useState('');
  const [marker, setMarker] = useState(false); const [x, setX] = useState(50); const [y, setY] = useState(50);
  const [winner, setWinner] = useState(''); const [filterName, setFilterName] = useState('Needs attention');
  const [filterState, setFilterState] = useState('changes_requested'); const [overdue, setOverdue] = useState(false); const [issuesOnly, setIssuesOnly] = useState(false);
  const [document, setDocument] = useState(''); const [tab, setTab] = useState('Review');
  const [pairImages, setPairImages] = useState<string[]>([]);
  const [available, setAvailable] = useState<ImageRowLite[]>([]); const [offset, setOffset] = useState(0);
  const identity = `${projectId}:${photoId}:${recipe?.recipeHash ?? ''}`;
  const selectionIdentity = `${identity}:${cohort}`; const current = useRef(selectionIdentity); current.current = selectionIdentity;
  const sequence = useRef(0);
  useEffect(() => { sequence.current += 1; setSnapshot(null); setBusy(false); setMessage(''); setError(''); setCohort([...new Set([photoId, ...photoIds])].slice(0, 32).join('\n')); setSource(photoId); setAvailable([]); setOffset(0); setDocument(''); setCheckpointId(''); setCriterion(''); setIssueId(''); setDependency(''); }, [identity]); // Selection changes are explicit through the cohort editor.
  useEffect(() => () => { current.current = ''; sequence.current += 1; }, []);
  useEffect(() => { sequence.current += 1; setBusy(false); setPairImages([]); setWinner(''); setMessage(''); setError(''); }, [identity, cohort]);
  const selected = [...new Set(cohort.split(/[\s,]+/).filter(Boolean))];
  const loaded = snapshot && selected.length === snapshot.photos.length && selected.every(id => snapshot.photos.some(p => p.photoId === id));
  const perform = async (action: WorkflowAction, extra: Partial<WorkflowRequest> = {}) => {
    if (disabled || busy || selected.length === 0 || selected.length > 32) return;
    const started = selectionIdentity; const request = ++sequence.current;
    setBusy(true); setError(''); setMessage('');
    try {
      // Reload must recover from a concurrent catalogue edit, so only mutations
      // carry the revision and photograph hashes of the loaded workspace.
      const result = await workflowLab.run({ projectId, action, photoIds: selected, reviewDate: new Intl.DateTimeFormat('en-CA').format(new Date()),
        ...(action === 'read' ? {} : { expectedRevision: snapshot?.revision, recipeHashes: Object.fromEntries(snapshot?.photos.map(p => [p.photoId, p.recipeHash]) ?? []) }), ...extra });
      if (current.current !== started || sequence.current !== request) return;
      setSnapshot(result); setMessage(result.message); if (result.document !== null) setDocument(result.document);
      if (action === 'read') setCriteria(result.criteria.join('\n'));
      if (action === 'apply_plan') await onEdited?.();
    } catch (failure) { if (current.current === started && sequence.current === request) setError(asIpcError(failure).message); }
    finally { if (current.current === started && sequence.current === request) setBusy(false); }
  };
  const button = (label: string, action: WorkflowAction, extra: Partial<WorkflowRequest> = {}, unavailable = false) =>
    <button type="button" disabled={disabled || busy || !loaded || unavailable} onClick={() => { void perform(action, extra); }}>{label}</button>;
  const selectedCheckpoint = snapshot?.checkpoints.find(saved => saved.id === checkpointId)?.id ?? snapshot?.checkpoints.at(-1)?.id ?? '';
  const selectedIssue = snapshot?.issues.find(issue => issue.id === issueId) ?? snapshot?.issues.at(-1);
  const chosenCriterion = snapshot?.criteria.includes(criterion) ? criterion : snapshot?.criteria[0] ?? '';
  const chosenSource = selected.includes(source) ? source : selected[0] ?? '';
  const chosenFilter = snapshot?.filters.find(filter => filter.name === filterName)?.name ?? snapshot?.filters[0]?.name ?? '';
  const browse = async (nextOffset: number) => {
    if (busy || disabled) return;
    const started = selectionIdentity; const request = ++sequence.current; setBusy(true); setError('');
    try {
      const images = await api.listImages({ projectId, offset: nextOffset, limit: 32, orderBy: null });
      if (current.current === started && sequence.current === request) { setAvailable(images); setOffset(nextOffset); }
    } catch (failure) { if (current.current === started && sequence.current === request) setError(asIpcError(failure).message); }
    finally { if (current.current === started && sequence.current === request) setBusy(false); }
  };
  const previewPair = async () => {
    if (!loaded || !snapshot || selected.length !== 2 || busy || disabled) return;
    const started = selectionIdentity; const request = ++sequence.current; setBusy(true); setError(''); setPairImages([]);
    const verify = () => workflowLab.run({ projectId, action: 'verify', photoIds: selected, recipeHashes: Object.fromEntries(snapshot.photos.map(p => [p.photoId, p.recipeHash])) });
    try {
      await verify();
      const frames = await Promise.all(selected.map(id => develop.renderImage({ photoId: id, level: 'screen', screen: [512, 512], purpose: 'export', colourSpace: 'srgb' })));
      await verify();
      if (current.current !== started || sequence.current !== request) return;
      const images = frames.map(rgbDataUrl); if (images.some(image => !image)) { setError('The comparison preview could not be displayed.'); return; }
      setPairImages(images as string[]);
    } catch (failure) { if (current.current === started && sequence.current === request) setError(asIpcError(failure).message); }
    finally { if (current.current === started && sequence.current === request) setBusy(false); }
  };
  return <section className="workflow-lab" aria-label="Workflow lab" aria-busy={busy}>
    <h3>Workflow lab</h3>
    <p>Review a cohort, preserve shared checkpoints, and plan safe batch edits. Checkpoint storage and review records stay in this catalogue.</p>
    <button type="button" disabled={disabled || busy} onClick={() => { void browse(0); }}>Choose collection photographs</button>
    {available.length > 0 && <fieldset disabled={disabled || busy}><legend>Choose up to 32 photographs</legend>
      {available.map(photo => <label key={photo.id} style={{ display: 'block' }}><input type="checkbox" checked={selected.includes(photo.id)} disabled={!selected.includes(photo.id) && selected.length >= 32}
        onChange={e => setCohort(e.target.checked ? [...selected, photo.id].join('\n') : selected.filter(id => id !== photo.id).join('\n'))} />{photo.fileName || photo.id.slice(0, 12)}</label>)}
      <button type="button" disabled={offset === 0} onClick={() => { void browse(Math.max(0, offset - 32)); }}>Previous photographs</button><button type="button" disabled={available.length < 32} onClick={() => { void browse(offset + 32); }}>More photographs</button>
    </fieldset>}
    <details><summary>Cohort order and identifiers</summary><label>Photograph IDs, one per line<textarea aria-label="Workflow photograph IDs" rows={3} value={cohort} disabled={disabled || busy} onChange={e => setCohort(e.target.value)} /></label></details>
    <p>{selected.length} of 32 photographs</p>
    <button type="button" disabled={disabled || busy || !selected.length || selected.length > 32} onClick={() => { void perform('read'); }}>{busy ? 'Working…' : 'Load review workspace'}</button>
    {error && <p role="alert">{error}</p>}{message && <p role="status">{message}</p>}
    {snapshot && <>
      <nav aria-label="Workflow sections">{['Review', 'Checkpoints', 'Compare', 'Handoff'].map(name => <button type="button" key={name} aria-pressed={tab === name} onClick={() => setTab(name)}>{name}</button>)}</nav>
      {!loaded && <p>Reload the workspace after changing the cohort.</p>}
      <table><thead><tr><th>Photo</th><th>Review</th><th>Issues</th><th>Due</th></tr></thead><tbody>{snapshot.photos.map(photo => <tr key={photo.photoId}>
        <td title={photo.photoId}>{photo.photoId.slice(0, 12)}</td><td>{photo.status.replaceAll('_', ' ')}</td><td>{photo.openIssues}</td><td>{photo.overdue ? 'Overdue' : snapshot.reviews[photo.photoId]?.dueDate || '—'}</td>
      </tr>)}</tbody></table>
      {tab === 'Review' && <>
        <fieldset disabled={disabled || busy || !loaded}><legend>Review queue & checklist</legend>
          {button('Save ordered cohort as review queue', 'queue')}
          {snapshot.queue.length > 0 && <><p>Saved queue: {snapshot.queue.map(id => id.slice(0, 8)).join(' → ')}</p><button type="button" onClick={() => setCohort(snapshot.queue.join('\n'))}>Use saved queue as cohort</button></>}
          <p>Queue order follows the ID list above. Edit that order and reload before saving it.</p>
          <label>Checklist criteria<textarea aria-label="Review checklist criteria" rows={3} value={criteria} onChange={e => setCriteria(e.target.value)} /></label>
          {button('Save checklist and restart signoffs', 'criteria', { text: criteria })}
          <label>Signoff criterion<select aria-label="Signoff criterion" value={chosenCriterion} onChange={e => setCriterion(e.target.value)}>{snapshot.criteria.map(name => <option key={name}>{name}</option>)}</select></label>
          {button('Pass criterion for cohort', 'signoff', { criterion: chosenCriterion, passed: true }, !chosenCriterion)}
          {button('Reopen criterion for cohort', 'signoff', { criterion: chosenCriterion, passed: false }, !chosenCriterion)}
          <p>Signoffs belong to the saved recipe revision. A later edit needs a new review.</p>
          <label>Reviewer label<input aria-label="Review owner" maxLength={80} value={reviewer} onChange={e => setReviewer(e.target.value)} /></label>
          {button('Assign selected review workload', 'assign', { reviewer }, !reviewer.trim())}
          <label>Review due date<input type="date" aria-label="Review due date" value={dueDate} onChange={e => setDueDate(e.target.value)} /></label>
          {button('Schedule cohort and order queue by due date', 'schedule', { dueDate })}
        </fieldset>
        <fieldset disabled={disabled || busy || !loaded}><legend>Correction issues & prerequisites</legend>
          <label>Correction note<textarea aria-label="Correction note" maxLength={500} value={text} onChange={e => setText(e.target.value)} /></label>
          <label><input type="checkbox" checked={marker} onChange={e => setMarker(e.target.checked)} />Anchor issue to photo coordinates</label>
          {marker && <><label>Marker X (%)<input type="number" min={0} max={100} value={x} onChange={e => setX(Number(e.target.value))} /></label><label>Marker Y (%)<input type="number" min={0} max={100} value={y} onChange={e => setY(Number(e.target.value))} /></label></>}
          <label>Prerequisite issue<select aria-label="Prerequisite issue" value={dependency} onChange={e => setDependency(e.target.value)}><option value="">None</option>{snapshot.issues.map(issue => <option key={issue.id} value={issue.id}>{issue.text}</option>)}</select></label>
          {button('Request correction for cohort', 'issue', { text, ...(marker ? { point: [x / 100, y / 100] as [number, number] } : {}), dependsOn: dependency ? [dependency] : [] }, !text.trim())}
          <label>Saved correction<select aria-label="Saved correction" value={selectedIssue?.id ?? ''} onChange={e => setIssueId(e.target.value)}>{snapshot.issues.map(issue => <option key={issue.id} value={issue.id}>{issue.resolved ? '✓ ' : ''}{issue.text} ({issue.photoId.slice(0, 8)})</option>)}</select></label>
          {selectedIssue?.point && <p>Marker: {(selectedIssue.point[0] * 100).toFixed(1)}%, {(selectedIssue.point[1] * 100).toFixed(1)}%</p>}
          {button('Resolve correction after prerequisites', 'resolve_issue', { issueId: selectedIssue?.id, passed: true }, !selectedIssue)}
          {button('Reopen correction', 'resolve_issue', { issueId: selectedIssue?.id, passed: false }, !selectedIssue)}
          {button('Propagate correction to other cohort photos', 'propagate_issue', { issueId: selectedIssue?.id }, !selectedIssue)}
          <p>Propagated notes omit the source coordinate marker. Dependencies must belong to the same photograph.</p>
        </fieldset>
        <fieldset disabled={disabled || busy || !loaded}><legend>Saved review filters</legend>
          <label>Filter name<input aria-label="Review filter name" value={filterName} maxLength={80} onChange={e => setFilterName(e.target.value)} /></label>
          <label>Review state<select aria-label="Review filter state" value={filterState} onChange={e => setFilterState(e.target.value)}>{STATES.map(state => <option value={state} key={state}>{state || 'Any state'}</option>)}</select></label>
          <label><input type="checkbox" checked={overdue} onChange={e => setOverdue(e.target.checked)} />Overdue only</label><label><input type="checkbox" checked={issuesOnly} onChange={e => setIssuesOnly(e.target.checked)} />Open issues only</label>
          <p>The reviewer label above also filters ownership when filled.</p>
          {button('Save custom review filter', 'filter', { filter: { name: filterName, reviewer, state: filterState, overdueOnly: overdue, issuesOnly } })}
          <label>Saved filter<select aria-label="Saved review filter" value={chosenFilter} onChange={e => setFilterName(e.target.value)}>{snapshot.filters.map(filter => <option key={filter.name}>{filter.name}</option>)}</select></label>
          {button('Find matching cohort photos', 'apply_filter', { name: chosenFilter }, !chosenFilter)}
        </fieldset>
      </>}
      {tab === 'Checkpoints' && <fieldset disabled={disabled || busy || !loaded}><legend>Cohort checkpoints & batch plans</legend>
        <label>Checkpoint name<input aria-label="Cohort checkpoint name" maxLength={80} value={name} onChange={e => setName(e.target.value)} /></label>{button('Capture cohort checkpoint', 'checkpoint', { name })}
        <label>Saved checkpoint<select aria-label="Saved cohort checkpoint" value={selectedCheckpoint} onChange={e => setCheckpointId(e.target.value)}>{snapshot.checkpoints.map(saved => <option key={saved.id} value={saved.id}>{saved.name} · {saved.count} photos</option>)}</select></label>
        {button('Compare cohort with checkpoint', 'drift', { checkpointId: selectedCheckpoint }, !selectedCheckpoint)}
        <label>Transfer source<select aria-label="Workflow transfer source" value={chosenSource} onChange={e => setSource(e.target.value)}>{selected.map(id => <option key={id} value={id}>{id.slice(0, 12)}</option>)}</select></label>
        <div role="group" aria-label="Workflow sections to change">{SECTIONS.map(section => <label key={section}><input type="checkbox" checked={sections.includes(section)} onChange={e => setSections(old => e.target.checked ? [...old, section] : old.filter(v => v !== section))} />{section.replaceAll('_', ' ')}</label>)}</div>
        {button('Plan section restore from checkpoint', 'restore_plan', { checkpointId: selectedCheckpoint, sections }, !selectedCheckpoint || !sections.length)}
        {button('Preview conflict-aware rollback', 'rollback_plan', { checkpointId: selectedCheckpoint, sections }, !selectedCheckpoint || !snapshot.plan)}
        {button('Plan relative tone and colour transfer', 'delta_plan', { checkpointId: selectedCheckpoint, sections, sourcePhotoId: chosenSource }, !selectedCheckpoint || !sections.length)}
        {button('Plan baseline-aware three-way transfer', 'merge_plan', { checkpointId: selectedCheckpoint, sections, sourcePhotoId: chosenSource }, !selectedCheckpoint || !sections.length)}
        <p>Relative transfer adds source changes since the checkpoint to each target. Three-way transfer keeps conflicting target sections. Geometry transfers only restore each photo's own checkpoint.</p>
        {snapshot.plan && <><h4>Prepared change manifest: {snapshot.plan.kind}</h4><Rows rows={snapshot.plan.entries} />
          {button('Apply reviewed batch plan', 'apply_plan', {}, snapshot.plan.entries.some(row => row.conflicts.length > 0))}
          <p>Each successful photo save has Undo history. Concurrent failures are reported per photo; this is not an all-or-nothing multi-photo edit.</p>
        </>}
        {button('Remove selected checkpoint', 'remove_checkpoint', { checkpointId: selectedCheckpoint }, !selectedCheckpoint)}
      </fieldset>}
      {tab === 'Compare' && <fieldset disabled={disabled || busy || !loaded}><legend>Saved-edit comparison & consistency</legend>
        {button('Verify all loaded saved revisions', 'verify')}{button('Group cohort by edit fingerprint', 'fingerprints')}{button('Show changed-section matrix', 'matrix')}
        <ul aria-label="Settings fingerprint cohorts">{[...new Set(snapshot.photos.map(photo => photo.fingerprint))].map(fingerprint => <li key={fingerprint}>Settings {fingerprint.slice(0, 10)}: {snapshot.photos.filter(photo => photo.fingerprint === fingerprint).map(photo => photo.photoId.slice(0, 8)).join(', ')}</li>)}</ul>
        <table><thead><tr><th>Photo</th><th>Fingerprint</th>{SECTIONS.map(name => <th key={name}>{name}</th>)}</tr></thead><tbody>{snapshot.photos.map(photo => <tr key={photo.photoId}><td>{photo.photoId.slice(0, 8)}</td><td>{photo.fingerprint.slice(0, 10)}</td>{SECTIONS.map(name => <td key={name}>{photo.sections[name]?.slice(0, 6)}</td>)}</tr>)}</tbody></table>
        <p>Matching fingerprints describe matching settings in these sections, not visually identical photographs. Select exactly two photos to save a preference.</p>
        <button type="button" disabled={disabled || busy || !loaded || selected.length !== 2} onClick={() => { void previewPair(); }}>Preview saved edit pair</button>
        {pairImages.length === 2 && <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>{pairImages.map((url, index) => <figure key={index} style={{ margin: 0 }}><img src={url} alt={index ? 'Right saved edit' : 'Left saved edit'} style={{ width: '100%' }} /><figcaption>{index ? 'Right' : 'Left'}</figcaption></figure>)}</div>}
        <label>Pair preference<select aria-label="Pair preference" value={winner} onChange={e => setWinner(e.target.value)}><option value="">Undecided</option><option value="tie">Tie</option>{selected.slice(0, 2).map((id, index) => <option key={id} value={id}>{index ? 'Right' : 'Left'} · {id.slice(0, 8)}</option>)}</select></label>
        <label>Comparison note<input aria-label="Comparison note" maxLength={500} value={text} onChange={e => setText(e.target.value)} /></label>
        {button('Record saved-revision pair preference', 'compare', { winner, text }, selected.length !== 2)}{button('Find tied, undecided or changed comparisons', 'undecided')}
        <ul>{snapshot.comparisons.map(pair => <li key={`${pair.left}:${pair.right}`}>{pair.left.slice(0, 8)} / {pair.right.slice(0, 8)}: {pair.winner || 'Undecided'} {pair.note}</li>)}</ul>
      </fieldset>}
      {tab === 'Handoff' && <fieldset disabled={disabled || busy || !loaded}><legend>Handoff & portable review</legend>
        {button('Build checklist-gated handoff manifest', 'handoff')}{button('Export review metadata', 'export')}
        <label>Review interchange or handoff document<textarea aria-label="Review interchange document" rows={7} value={document} onChange={e => setDocument(e.target.value)} /></label>
        {button('Import review metadata and reopen approvals', 'import', { document }, !document.trim())}
        <p>Import replaces review metadata, reopens imported corrections and requires new signoffs. Select all referenced photos. It does not import recipes or send messages.</p>
        {button('Read workflow activity history', 'audit')}<ol>{snapshot.audit.slice().reverse().map((entry, index) => <li key={index}>{entry.at} · {entry.action} · {entry.photos.length} photos</li>)}</ol>
      </fieldset>}
      {snapshot.rows.length > 0 && <Rows rows={snapshot.rows} />}
      {snapshot.selected.length > 0 && <><p>Matching photos: {snapshot.selected.length}</p><textarea aria-label="Matching review photograph IDs" readOnly value={snapshot.selected.join('\n')} /></>}
    </>}
  </section>;
}
