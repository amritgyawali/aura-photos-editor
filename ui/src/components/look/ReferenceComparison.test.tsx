import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import type { ColorReference } from '../../ipc/colorMatch';
import { ReferenceComparison } from './ReferenceComparison';

const reference: ColorReference = { id: 'look', name: 'Warm wedding', width: 800, height: 1200,
  convertedIcc: false, regions: ['overall'], thumbnail: 'data:image/png;base64,thumb', preview: 'data:image/jpeg;base64,preview' };

it('shows the selected display reference beside the active photo and preserves the active image across modes', () => {
  const view = render(<ReferenceComparison enabled={false} reference={reference}><img src="active.jpg" alt="Active pixels" /></ReferenceComparison>);
  const active = screen.getByAltText('Active pixels');
  expect(screen.queryByRole('figure', { name: 'Colour reference preview' })).toBeNull();
  view.rerender(<ReferenceComparison enabled reference={reference}><img src="active.jpg" alt="Active pixels" /></ReferenceComparison>);
  expect(screen.getByAltText('Colour reference: Warm wedding').getAttribute('src')).toBe(reference.preview);
  expect(screen.getByAltText('Active pixels')).toBe(active);
  view.rerender(<ReferenceComparison enabled={false} reference={reference}><img src="active.jpg" alt="Active pixels" /></ReferenceComparison>);
  expect(screen.getByAltText('Active pixels')).toBe(active);
});

it('handles descriptor-only and failed previews without inventing a reference image', () => {
  const view = render(<ReferenceComparison enabled reference={{ ...reference, preview: undefined, thumbnail: undefined }}><span>Active pixels</span></ReferenceComparison>);
  expect(screen.getByText('Reference preview unavailable')).toBeTruthy();
  expect(screen.queryByAltText('Colour reference: Warm wedding')).toBeNull();
  view.rerender(<ReferenceComparison enabled reference={reference}><span>Active pixels</span></ReferenceComparison>);
  fireEvent.error(screen.getByAltText('Colour reference: Warm wedding'));
  expect(screen.getByText('Reference preview unavailable')).toBeTruthy();
});

it('synchronizes zoom and normalized pan between different pane sizes, with an independent mode', () => {
  render(<ReferenceComparison enabled reference={reference}><span>Active pixels</span></ReferenceComparison>);
  fireEvent.change(screen.getByLabelText('Comparison zoom'), { target: { value: '2' } });
  const left = screen.getByLabelText('Pan reference photograph');
  const right = screen.getByLabelText('Pan active photograph');
  expect((left.firstElementChild as HTMLElement).style.width).toBe('200%');
  expect((right.firstElementChild as HTMLElement).style.width).toBe('200%');
  Object.defineProperties(left, { scrollWidth: { value: 400 }, clientWidth: { value: 200 }, scrollHeight: { value: 600 }, clientHeight: { value: 300 } });
  Object.defineProperties(right, { scrollWidth: { value: 600 }, clientWidth: { value: 200 }, scrollHeight: { value: 900 }, clientHeight: { value: 300 } });
  left.scrollLeft = 100; left.scrollTop = 150; fireEvent.scroll(left);
  expect(right.scrollLeft).toBe(200); expect(right.scrollTop).toBe(300);
  fireEvent.click(screen.getByLabelText('Sync zoom and pan'));
  fireEvent.change(screen.getByLabelText('Reference zoom'), { target: { value: '1.5' } });
  expect((left.firstElementChild as HTMLElement).style.width).toBe('150%');
  expect((right.firstElementChild as HTMLElement).style.width).toBe('200%');
  left.scrollLeft = 0; fireEvent.scroll(left); expect(right.scrollLeft).toBe(200);
});
