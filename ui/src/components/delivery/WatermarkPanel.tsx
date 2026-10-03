import { useEffect, useState } from 'react';
import type { ExportWatermark } from '../../ipc/client';

export type WatermarkSettings = { enabled: boolean; text: string; color: string; opacity: number; width: number; margin: number; anchor: ExportWatermark['anchor']; logo: { width: number; height: number; rgba: number[] } | null };
export const defaultWatermark: WatermarkSettings = { enabled: false, text: '', color: '#ffffff', opacity: 70, width: 25, margin: 3, anchor: 'bottom_right', logo: null };

/** Freeze the user's text/logo as bounded sRGB pixels; native export owns compositing. */
export function prepareWatermark(settings: WatermarkSettings): ExportWatermark {
  let graphic = settings.logo;
  if (!graphic) {
    const text = settings.text.trim();
    if (!text) throw new Error('Enter watermark text or choose a PNG logo.');
    const canvas = document.createElement('canvas');
    canvas.width = 1024; canvas.height = 128;
    const ctx = canvas.getContext('2d', { colorSpace: 'srgb' });
    if (!ctx) throw new Error('Watermark drawing is unavailable.');
    ctx.font = '64px sans-serif';
    const scale = Math.min(1, 1000 / Math.max(1, ctx.measureText(text).width));
    ctx.font = `${64 * scale}px sans-serif`;
    const width = Math.min(1024, Math.ceil(ctx.measureText(text).width) + 24);
    canvas.width = width; canvas.height = Math.ceil(90 * scale) + 16;
    ctx.font = `${64 * scale}px sans-serif`; ctx.fillStyle = settings.color; ctx.textBaseline = 'middle';
    ctx.fillText(text, 12, canvas.height / 2);
    graphic = { width: canvas.width, height: canvas.height, rgba: Array.from(ctx.getImageData(0, 0, canvas.width, canvas.height).data) };
  }
  return { ...graphic, opacity: settings.opacity / 100, widthFraction: settings.width / 100, marginFraction: settings.margin / 100, anchor: settings.anchor };
}

export function WatermarkPanel({ value, onChange, disabled, onReadingChange }: { value: WatermarkSettings; onChange: (value: WatermarkSettings) => void; disabled: boolean; onReadingChange: (reading: boolean) => void }): JSX.Element {
  const [error, setError] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  useEffect(() => { onReadingChange(reading); return () => onReadingChange(false); }, [reading, onReadingChange]);
  const update = (change: Partial<WatermarkSettings>) => onChange({ ...value, ...change });
  return <details className="lr-section watermark-panel"><summary>Export watermark</summary><fieldset disabled={disabled || reading} className="lr-section-body">
    <label className="lr-toggle"><input type="checkbox" checked={value.enabled} onChange={event => update({ enabled: event.target.checked })} />Add watermark to exported photos</label>
    {value.enabled && <>
      <label>Watermark text<input maxLength={120} value={value.text} disabled={Boolean(value.logo)} onChange={event => update({ text: event.target.value })} /></label>
      <label>Text color<input type="color" value={value.color} disabled={Boolean(value.logo)} onChange={event => update({ color: event.target.value })} /></label>
      <label>PNG logo<input type="file" accept="image/png" onChange={event => {
        const file = event.target.files?.[0]; if (!file) return;
        setError(null); setReading(true);
        void (async () => {
          if (file.type !== 'image/png' || file.size > 5_000_000) throw new Error('Choose a PNG logo smaller than 5 MB.');
          const bitmap = await createImageBitmap(file);
          try {
            const scale = Math.min(1, 1024 / bitmap.width, 256 / bitmap.height);
            const canvas = document.createElement('canvas');
            canvas.width = Math.max(1, Math.round(bitmap.width * scale)); canvas.height = Math.max(1, Math.round(bitmap.height * scale));
            const ctx = canvas.getContext('2d', { colorSpace: 'srgb' });
            if (!ctx) throw new Error('Logo drawing is unavailable.');
            ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
            update({ logo: { width: canvas.width, height: canvas.height, rgba: Array.from(ctx.getImageData(0, 0, canvas.width, canvas.height).data) } });
          } finally { bitmap.close(); }
        })().catch(cause => setError(cause instanceof Error ? cause.message : 'Cannot read this logo.')).finally(() => setReading(false));
      }} /></label>
      {value.logo && <button type="button" onClick={() => update({ logo: null })}>Remove logo; use text</button>}
      <label>Position<select value={value.anchor} onChange={event => update({ anchor: event.target.value as ExportWatermark['anchor'] })}>
        <option value="bottom_right">Bottom right</option><option value="bottom_left">Bottom left</option><option value="top_right">Top right</option><option value="top_left">Top left</option><option value="center">Center</option>
      </select></label>
      <label>Opacity ({value.opacity}%)<input type="range" min={1} max={100} value={value.opacity} onChange={event => update({ opacity: Number(event.target.value) })} /></label>
      <label>Width ({value.width}% of photo)<input type="range" min={5} max={80} value={value.width} onChange={event => update({ width: Number(event.target.value) })} /></label>
      <label>Margin ({value.margin}%)<input type="range" min={0} max={10} value={value.margin} onChange={event => update({ margin: Number(event.target.value) })} /></label>
      <p className="lr-hint">Applied after resizing to every exported photo. Originals and saved edits stay untouched.</p>
    </>}
    {reading && <p role="status">Reading logo…</p>}{error && <p role="alert">{error}</p>}
  </fieldset></details>;
}
