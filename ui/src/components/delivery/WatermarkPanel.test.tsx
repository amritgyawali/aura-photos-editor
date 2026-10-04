import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { defaultWatermark, prepareWatermark, WatermarkPanel } from './WatermarkPanel';

afterEach(() => vi.unstubAllGlobals());

it('blocks delivery while a chosen logo is decoding and releases it on failure', async () => {
  let rejectDecode: (error: Error) => void = () => {};
  vi.stubGlobal('createImageBitmap', vi.fn(() => new Promise((_, reject) => { rejectDecode = reject; })));
  const reading = vi.fn();
  const changed = vi.fn();
  render(<WatermarkPanel value={{ ...defaultWatermark, enabled: true }} onChange={changed} disabled={false} onReadingChange={reading} />);
  fireEvent.click(screen.getByText('Export watermark'));
  fireEvent.change(screen.getByLabelText('PNG logo'), { target: { files: [new File(['bad png'], 'logo.png', { type: 'image/png' })] } });
  await waitFor(() => expect(reading).toHaveBeenLastCalledWith(true));
  expect(screen.getByLabelText('PNG logo').closest('fieldset')?.disabled).toBe(true);
  await act(async () => rejectDecode(new Error('Unreadable PNG')));
  expect((await screen.findByRole('alert')).textContent).toBe('Unreadable PNG');
  await waitFor(() => expect(reading).toHaveBeenLastCalledWith(false));
  expect(changed).not.toHaveBeenCalled();
});

it('rejects an enabled empty mark and submits a selected logo with normalized placement', () => {
  expect(() => prepareWatermark({ ...defaultWatermark, enabled: true })).toThrow('Enter watermark text');
  const graphic = { width: 1, height: 1, rgba: [30, 50, 70, 128] };
  expect(prepareWatermark({ ...defaultWatermark, enabled: true, logo: graphic })).toEqual({
    ...graphic, opacity: 0.7, widthFraction: 0.25, marginFraction: 0.03, anchor: 'bottom_right',
  });
});
