import { useEffect, useMemo, useRef, useState } from 'react';
import { asIpcError, inTauri } from '../../ipc/client';
import { editProfiles, swatchGradient, type EditProfile, type ProfilePreview, type ProfileSelection } from './profileSelection';

type Props = {
  selection: ProfileSelection | null;
  disabled: boolean;
  onChange: (selection: ProfileSelection | null) => void;
  /** A photograph to preview the chosen profile on; the built-in sample scene otherwise. */
  previewPhotoId?: string | null;
};

const ALL = 'All';

/**
 * Step one of the start screen: pick a look. Every card is rendered by the export renderer on the
 * same sample scene, so the differences you see are the differences you will get. The chosen
 * profile opens a larger before/after - on your own photo once one is imported.
 */
export function ProfileGallery({ selection, disabled, onChange, previewPhotoId }: Props): JSX.Element {
  const [profiles, setProfiles] = useState<EditProfile[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [category, setCategory] = useState(ALL);
  const [thumbs, setThumbs] = useState<Record<string, string>>({});
  const [detail, setDetail] = useState<ProfilePreview | null>(null);
  const [detailBusy, setDetailBusy] = useState(false);
  const [split, setSplit] = useState(50);
  const [strength, setStrength] = useState(selection?.strength ?? 1);
  const generation = useRef(0);

  useEffect(() => {
    if (!inTauri()) return;
    editProfiles.list().then(setProfiles).catch(cause => setError(asIpcError(cause).message));
  }, []);

  // Card previews, one at a time so the gallery never competes with an import for the renderer.
  useEffect(() => {
    if (!profiles.length) return;
    let cancelled = false;
    void (async () => {
      for (const profile of profiles) {
        if (cancelled) return;
        try {
          const preview = await editProfiles.preview(profile.id, 1, null, 240);
          if (!cancelled) setThumbs(current => ({ ...current, [profile.id]: preview.after }));
        } catch { /* The swatch stays; a missing thumbnail is not an error worth a banner. */ }
      }
    })();
    return () => { cancelled = true; };
  }, [profiles]);

  const chosen = profiles.find(profile => profile.id === selection?.profileId) ?? null;
  useEffect(() => { if (selection) setStrength(selection.strength); }, [selection]);

  // The large before/after follows the chosen profile, its strength and the photo in view.
  useEffect(() => {
    if (!chosen || !inTauri()) { setDetail(null); return; }
    const mine = ++generation.current;
    setDetailBusy(true);
    const timer = window.setTimeout(() => {
      editProfiles.preview(chosen.id, strength, previewPhotoId ?? null, 560)
        .then(preview => { if (mine === generation.current) setDetail(preview); })
        .catch(cause => { if (mine === generation.current) setError(asIpcError(cause).message); })
        .finally(() => { if (mine === generation.current) setDetailBusy(false); });
    }, 180);
    return () => window.clearTimeout(timer);
  }, [chosen, strength, previewPhotoId]);

  const categories = useMemo(() => [ALL, ...Array.from(new Set(profiles.map(p => p.category)))], [profiles]);
  const shown = profiles.filter(profile => category === ALL || profile.category === category);

  return <section className="profile-gallery" aria-label="Edit profiles">
    <header className="step-heading"><span className="step-number">1</span><div>
      <span className="eyebrow">CHOOSE YOUR LOOK</span>
      <h2>Pick an edit profile</h2>
      <p>{profiles.length ? `${profiles.length} profiles` : 'Profiles'} built from professional before-and-after edits. Each one adapts to every photo: AURA measures the light first, then adds the look, and softens it where a frame cannot take it.</p>
    </div></header>
    {!inTauri() && <p className="reference-note">Open the AURA desktop app to browse and apply edit profiles.</p>}
    {error && <p role="alert" className="reference-error">{error}</p>}
    {categories.length > 1 && <div className="profile-filters" role="group" aria-label="Profile categories">
      {categories.map(name => <button key={name} type="button" aria-pressed={category === name} onClick={() => setCategory(name)}>{name}</button>)}
    </div>}
    <div className="profile-grid" role="radiogroup" aria-label="Edit profile">
      <button type="button" role="radio" aria-checked={selection === null} className="profile-card" disabled={disabled} onClick={() => onChange(null)}>
        <span className="profile-thumb profile-thumb-auto" aria-hidden="true">A</span>
        <strong>Auto only</strong><span>Measured light and contrast, no creative look.</span>
      </button>
      {shown.map(profile => <button key={profile.id} type="button" role="radio" aria-checked={selection?.profileId === profile.id}
        className="profile-card" disabled={disabled} onClick={() => onChange({ profileId: profile.id, strength })}>
        <span className="profile-thumb" style={thumbs[profile.id] ? undefined : { background: swatchGradient(profile.swatch) }}>
          {thumbs[profile.id] && <img src={thumbs[profile.id]} alt="" />}
          <em className={`profile-origin is-${profile.origin}`}>{profile.origin === 'learned' ? 'Learned' : profile.category}</em>
        </span>
        <strong>{profile.name}</strong><span>{profile.tagline}</span>
      </button>)}
    </div>
    {chosen && <div className="profile-detail">
      <div className="profile-compare" aria-busy={detailBusy}>
        {detail ? <>
          <img src={detail.before} alt={`Before ${chosen.name}`} />
          <img src={detail.after} alt={`After ${chosen.name}`} style={{ clipPath: `inset(0 0 0 ${split}%)` }} />
          <span className="studio-divider" style={{ left: `${split}%` }} />
          <span className="profile-compare-label is-before">Before</span><span className="profile-compare-label is-after">After</span>
        </> : <span className="profile-compare-empty" style={{ background: swatchGradient(chosen.swatch) }} />}
        <label className="compare-control profile-split">Compare<input type="range" min={0} max={100} value={split} onChange={event => setSplit(Number(event.target.value))} aria-label="Before and after divider" /></label>
      </div>
      <div className="profile-about">
        <span className="eyebrow">{chosen.origin === 'learned' ? 'LEARNED FROM REAL EDITS' : chosen.category.toUpperCase()}</span>
        <h3>{chosen.name}</h3>
        <p>{chosen.description}</p>
        <p className="profile-best">Best for: {chosen.bestFor.join(' · ')}</p>
        <label className="reference-strength">Profile strength <output>{Math.round(strength * 100)}%</output>
          <input type="range" min={0} max={150} value={Math.round(strength * 100)} disabled={disabled}
            onChange={event => setStrength(Number(event.target.value) / 100)}
            onPointerUp={() => onChange({ profileId: chosen.id, strength })} onKeyUp={() => onChange({ profileId: chosen.id, strength })} />
        </label>
        {detail && detail.adaptations.length > 0 && <ul className="profile-adaptations">{detail.adaptations.map(note => <li key={note}>{note}</li>)}</ul>}
        {chosen.evidence && <p className="profile-evidence">Measured on {chosen.evidence.heldOutPairs} held-out RAW photos it never saw: difference from the retoucher’s final {chosen.evidence.autoDe00.toFixed(1)} → {chosen.evidence.profileDe00.toFixed(1)} ΔE00 (lower is closer). Learned from {chosen.evidence.trainingPairs} pairs · {chosen.evidence.dataset}.</p>}
        <details className="profile-technique"><summary>How this look is built</summary>
          <ol>{chosen.technique.map(step => <li key={step}>{step}</li>)}</ol>
          {chosen.sources.length > 0 && <p className="reference-note">Sources: {chosen.sources.map((source, index) => <span key={source.url}>{index > 0 && ' · '}<a href={source.url} target="_blank" rel="noreferrer">{source.title}</a></span>)}</p>}
        </details>
      </div>
    </div>}
  </section>;
}
