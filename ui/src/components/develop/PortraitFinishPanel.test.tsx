import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';

import { NEUTRAL_FINISH, finishOf, hexToRgb, normaliseFinish, rgbToHex } from '../../ipc/studioFinish';
import { PortraitFinishPanel } from './PortraitFinishPanel';

afterEach(cleanup);

it('a face slider commits one labelled save when released', () => {
  const onSave = vi.fn();
  render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={onSave} onLiquify={vi.fn()} />);
  const slider = screen.getByLabelText('Eye size');
  fireEvent.change(slider, { target: { value: '30' } });
  expect(onSave).not.toHaveBeenCalled();
  fireEvent.pointerUp(slider);
  expect(onSave).toHaveBeenCalledTimes(1);
  expect(onSave.mock.calls[0]?.[0].face.eyeSize).toBe(30);
  expect(onSave.mock.calls[0]?.[1]).toBe('Face: Eye size');
});

it('a background preset replaces the background and liquify picks a brush', () => {
  const onSave = vi.fn();
  const onLiquify = vi.fn();
  render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={onSave} onLiquify={onLiquify} />);
  fireEvent.click(screen.getByRole('button', { name: 'Clean white backdrop' }));
  expect(onSave.mock.calls[0]?.[0].background).toMatchObject({ mode: 'colour', colour: [1, 1, 1], amount: 100 });
  fireEvent.click(screen.getByRole('tab', { name: 'Liquify' }));
  fireEvent.click(screen.getByRole('button', { name: 'Enlarge' }));
  expect(onLiquify).toHaveBeenCalledWith(expect.objectContaining({ mode: 'bloat' }));
});

it('the panel says plainly that skin tone is never changed and nothing is automatic', () => {
  render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={vi.fn()} onLiquify={vi.fn()} />);
  expect(screen.getByText(/none of them is ever applied automatically/)).toBeTruthy();
  fireEvent.click(screen.getByRole('tab', { name: 'Makeup & colour' }));
  expect(screen.getByText(/Skin tone is never changed/)).toBeTruthy();
});

it('reads a stored finish out of a recipe and fills the gaps', () => {
  const body = JSON.stringify({ studio_finish_v1: { face: { slim: 20 }, colours: { lips: { colour: [0.7, 0.2, 0.2], amount: 40 } } } });
  const finish = finishOf({ body } as never);
  expect(finish.face.slim).toBe(20);
  expect(finish.face.eyeSize).toBe(0);
  expect(finish.colours.lips?.amount).toBe(40);
  expect(finish.colours.hair).toBeNull();
  expect(normaliseFinish(null)).toEqual(NEUTRAL_FINISH);
  expect(rgbToHex(hexToRgb('#7a3b1e'))).toBe('#7a3b1e');
});
