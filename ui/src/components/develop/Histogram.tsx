import { useMemo } from 'react';
import type { RenderDto } from '../../ipc/types';

/** Display-space channel distribution. Analysis is bounded to 100k preview pixels. */
export function measureHistogram(render: Pick<RenderDto, 'width' | 'height' | 'rgbBase64'>) {
  const pixels = render.width * render.height;
  if (!Number.isSafeInteger(pixels) || pixels < 1) throw new Error('Invalid histogram dimensions');
  const rgb = atob(render.rgbBase64);
  if (rgb.length !== pixels * 3) throw new Error('Incomplete histogram pixels');
  const channels = [new Uint32Array(64), new Uint32Array(64), new Uint32Array(64)];
  const stride = Math.max(1, Math.ceil(pixels / 100_000));
  let shadows = 0, highlights = 0, samples = 0;
  for (let pixel = 0; pixel < pixels; pixel += stride) {
    const r = rgb.charCodeAt(pixel * 3), g = rgb.charCodeAt(pixel * 3 + 1), b = rgb.charCodeAt(pixel * 3 + 2);
    channels[0]![r >> 2]!++; channels[1]![g >> 2]!++; channels[2]![b >> 2]!++;
    if (Math.max(r, g, b) <= 2) shadows++;
    if (Math.max(r, g, b) >= 253) highlights++;
    samples++;
  }
  return { channels, shadows: shadows / samples * 100, highlights: highlights / samples * 100 };
}

export function Histogram({ render }: { render: RenderDto }): JSX.Element {
  const measured = useMemo(() => measureHistogram(render), [render]);
  const peak = Math.max(1, ...measured.channels.flatMap(channel => Array.from(channel)));
  return <figure className="studio-histogram" aria-label="Edited photo RGB histogram">
    <figcaption><strong>Light distribution</strong><span>Edited · RGB</span></figcaption>
    <svg viewBox="0 0 252 64" role="img" aria-label="Red, green and blue channels from shadows to highlights" preserveAspectRatio="none">
      {[63, 126, 189].map(x => <line key={x} x1={x} x2={x} y1={0} y2={64} stroke="currentColor" opacity=".15" />)}
      {measured.channels.map((channel, index) => <path key={index} fill={['#ed9398', '#9ed7b0', '#a4bafa'][index]} fillOpacity=".32"
        stroke={['#ed9398', '#9ed7b0', '#a4bafa'][index]} strokeWidth=".8"
        d={`M0,64 ${Array.from(channel, (count, bin) => `L${bin * 4},${64 - count / peak * 60}`).join(' ')} L252,64 Z`} />)}
    </svg>
    <div className="histogram-extremes"><span>Near black <b>{measured.shadows.toFixed(1)}%</b></span><span>Near white <b>{measured.highlights.toFixed(1)}%</b></span></div>
    <p>Preview sample. Near white includes any channel close to clipping.</p>
  </figure>;
}
