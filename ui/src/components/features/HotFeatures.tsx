import { useId } from 'react';
import { useThumbnails } from '../../stores/thumbnailStore';
import './hotFeatures.css';

export const HOT_FEATURES = [
  { id: 'liquify', title: 'Liquify Background Repair', description: 'Restore background lines distorted by saved shape and liquify edits.', icon: 'M4 4h16v16H4z M4 10c5-5 11 5 16 0 M10 4c-5 5 5 11 0 16' },
  { id: 'skin', title: 'Face Freckles & Acne', description: 'Refine spots and freckles with local skin repair and texture controls.', icon: 'M12 3a9 9 0 1 0 9 9 M15 4l5 5 M7 9h.01 M11 14h.01 M8 17h.01 M16 15h.01' },
  { id: 'glare', title: 'Remove Glasses Glare', description: 'Soften lens reflections while keeping visible eye detail.', icon: 'M3 13a4 4 0 1 0 8 0a4 4 0 1 0-8 0 M13 13a4 4 0 1 0 8 0a4 4 0 1 0-8 0 M11 12h2 M17 3v5 M14.5 5.5h5' },
  { id: 'train', title: 'Train Your AI Look', description: 'Learn a personal colour profile from your originals and finished edits.', icon: 'M3 4h18v16H3z M5 18l5-6 4 4 3-3 4 5 M16 6v5 M13.5 8.5h5' },
  { id: 'color', title: 'AI Color Match', description: 'Match a reference photo’s colour and tone across your collection.', icon: 'M3 6h18 M3 12h18 M3 18h18 M8 3v6 M16 9v6 M10 15v6' },
] as const;
export type HotFeature = typeof HOT_FEATURES[number]['id'];

/** Local artwork while no photograph is selected. It does not pretend to be an edited result. */
function FeatureArtwork({ index }: { index: number }) {
  const id = useId();
  return <svg className={`hot-art hot-art-${index}`} viewBox="0 0 300 170" aria-hidden="true">
    <defs><linearGradient id={id} x2="1" y2="1"><stop stopColor={['#80958c', '#c2b39b', '#999da6', '#bea7a2', '#6f9b9c'][index]} /><stop offset="1" stopColor="#343b40" /></linearGradient></defs>
    <rect width="300" height="170" fill={`url(#${id})`} />
    <path d="M20 0v170M55 0v170M90 0v170M225 0v170M260 0v170" stroke="#fff" strokeOpacity=".14" />
    <ellipse cx="160" cy="185" rx="83" ry="67" fill="#dedbd5" />
    <path d="M112 101Q95 7 158 9Q219 7 208 115L182 137H137Z" fill="#3e302d" />
    <path d="M143 95h34l6 45q-22 19-47 0Z" fill="#be967c" />
    <ellipse cx="160" cy="70" rx="37" ry="48" fill="#d8b399" />
    <path d="M124 60Q116 17 159 14Q198 10 202 65Q175 45 169 29Q148 54 124 60Z" fill="#44332e" />
    <path d="M138 70h10m24 0h10m-29 28q8 5 16 0" stroke="#75584b" strokeWidth="2" strokeLinecap="round" fill="none" />
    {index === 2 && <g fill="none" stroke="#e7ce74" strokeWidth="4"><rect x="129" y="58" width="28" height="23" rx="8" /><rect x="166" y="58" width="28" height="23" rx="8" /><path d="M157 65h9" /></g>}
    {index === 1 && <g fill="#ac7964">{[[136, 83], [143, 88], [177, 85], [185, 88], [145, 80]].map(([cx, cy], key) => <circle key={key} cx={cx} cy={cy} r="1.4" />)}</g>}
    {index === 4 && <rect width="150" height="170" fill="#e5b86a" opacity=".15" />}
  </svg>;
}

export function HotFeatures({ selected, onSelect, disabled, photoId, hasPhotos, onAddPhotos }: {
  selected: HotFeature; onSelect: (feature: HotFeature) => void; disabled: boolean;
  photoId: string | null; hasPhotos: boolean; onAddPhotos: () => void;
}) {
  const thumbnail = useThumbnails(state => photoId ? state.entries.get(photoId)?.dataUrl : undefined);
  return <section className="hot-features" aria-label="Hot Features">
    <header className="workspace-heading"><div><span className="eyebrow">YOUR CREATIVE TOOLKIT</span><h1>Hot Features</h1><p>Choose a tool, select a photo, and make it yours. Saved edits stay available in History and Export.</p></div></header>
    <div className="hot-feature-grid" aria-label="Feature tools">
      {HOT_FEATURES.map((feature, index) => <button type="button" key={feature.id} className="hot-feature-card" aria-pressed={selected === feature.id}
        disabled={disabled} onClick={() => onSelect(feature.id)} aria-label={feature.title}>
        <span className="hot-feature-image">{thumbnail ? <img src={thumbnail} alt="" /> : <FeatureArtwork index={index} />}
          <span className="hot-feature-icon"><svg viewBox="0 0 24 24" aria-hidden="true"><path d={feature.icon} /></svg></span>
        </span>
        <span className="hot-feature-copy"><strong>{feature.title}</strong><span>{feature.description}</span><span className="hot-feature-open">{selected === feature.id ? 'Tool open ↓' : 'Open tool →'}</span></span>
      </button>)}
    </div>
    {!hasPhotos && <div className="hot-feature-empty"><strong>Add your photos to begin.</strong><p>Choose the photos you want to work on. Importing here leaves them ready for your chosen tool.</p><button type="button" disabled={disabled} onClick={onAddPhotos}>Add photos for Hot Features</button></div>}
  </section>;
}
