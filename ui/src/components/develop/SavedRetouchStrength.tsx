import { useEffect, useState } from 'react';

/** Absolute operation strength: repeated changes never multiply the previous value. */
export function SavedRetouchStrength({ index, amount, disabled, onApply }: {
  index: number; amount: number; disabled: boolean; onApply: (amount: number) => void;
}): JSX.Element {
  const [draft, setDraft] = useState(amount);
  useEffect(() => { setDraft(amount); }, [amount]);
  const changed = Math.abs(draft - amount) > 0.00001;
  return <fieldset className="saved-retouch-strength" disabled={disabled}>
    <legend>Adjust saved strength</legend>
    <label>Saved strength ({Math.round(draft * 100)}%)
      <input type="range" aria-label={`Retouch ${index} strength`} min="0" max="100" step="1"
        value={Math.round(draft * 100)} onChange={event => setDraft(Number(event.target.value) / 100)}/>
    </label>
    <button type="button" aria-label={`Apply retouch ${index} strength`} disabled={!changed}
      onClick={() => onApply(draft)}>Apply strength</button>
    {changed && <button type="button" aria-label={`Discard retouch ${index} strength`}
      onClick={() => setDraft(amount)}>Discard</button>}
  </fieldset>;
}
