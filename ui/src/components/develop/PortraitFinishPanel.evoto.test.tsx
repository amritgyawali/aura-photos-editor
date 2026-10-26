import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';

import { NEUTRAL_FINISH, finishOf } from '../../ipc/studioFinish';
import { PortraitFinishPanel } from './PortraitFinishPanel';

afterEach(cleanup);

it('every ADR-0150 group saves its own slider with a labelled step', () => {
  const onSave = vi.fn();
  render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={onSave} onLiquify={vi.fn()} />);
  const drag = (label: string, value: string) => {
    const slider = screen.getByLabelText(label);
    fireEvent.change(slider, { target: { value } });
    fireEvent.pointerUp(slider);
  };
  drag('Eye tilt', '20');
  expect(onSave.mock.lastCall?.[0].face.eyeTilt).toBe(20);
  fireEvent.click(screen.getByRole('tab', { name: 'Body' }));
  drag('Thighs', '35');
  expect(onSave.mock.lastCall?.[0].body.thighs).toBe(35);
  fireEvent.click(screen.getByRole('tab', { name: 'Skin' }));
  drag('Aegyo-sal (eye smile)', '40');
  expect(onSave.mock.lastCall?.[0].skin.aegyoSal).toBe(40);
  expect(onSave.mock.lastCall?.[1]).toBe('Skin: Aegyo-sal (eye smile)');
  fireEvent.click(screen.getByRole('tab', { name: 'Makeup & colour' }));
  drag('Eyeliner', '50');
  expect(onSave.mock.lastCall?.[0].makeup.eyeliner).toBe(50);
  fireEvent.click(screen.getByRole('tab', { name: 'Hair & attire' }));
  drag('Gold jewellery', '60');
  expect(onSave.mock.lastCall?.[0].attire.gold).toBe(60);
  drag('Hair shine', '25');
  expect(onSave.mock.lastCall?.[0].hair.gloss).toBe(25);
  fireEvent.click(screen.getByRole('tab', { name: 'People & scene' }));
  drag('Background saturation', '-40');
  expect(onSave.mock.lastCall?.[0].scene.saturation).toBe(-40);
  drag('Balance group faces', '70');
  expect(onSave.mock.lastCall?.[0].subject.groupBalance).toBe(70);
  // Saving one group never drops another.
  expect(onSave.mock.lastCall?.[0].skin).toEqual(NEUTRAL_FINISH.skin);
});

it('a stored finish keeps every ADR-0150 group when an older slider is saved', () => {
  const onSave = vi.fn();
  const finish = { ...NEUTRAL_FINISH, attire: { ...NEUTRAL_FINISH.attire, gold: 55 }, faces: [[0.4, 0.3]] as [number, number][] };
  render(<PortraitFinishPanel finish={finish} disabled={false} liquify={null} onSave={onSave} onLiquify={vi.fn()} />);
  const slider = screen.getByLabelText('Eye size');
  fireEvent.change(slider, { target: { value: '10' } });
  fireEvent.pointerUp(slider);
  expect(onSave.mock.lastCall?.[0].attire.gold).toBe(55);
  expect(onSave.mock.lastCall?.[0].faces).toEqual([[0.4, 0.3]]);
  const body = JSON.stringify({ studio_finish_v1: { skin: { glow: 30 }, faces: [[0.5, 0.5]] } });
  const read = finishOf({ body } as never);
  expect(read.skin.glow).toBe(30);
  expect(read.subject.relightDirection).toBe(-45);
  expect(read.faces).toEqual([[0.5, 0.5]]);
});

it('the face picker lists the faces found and saves the chosen ones', async () => {
  const onSave = vi.fn();
  const onFindFaces = vi.fn().mockResolvedValue([
    { bounds: [0.1, 0.2, 0.3, 0.4], centre: [0.2, 0.3], tilt: 0, eyes: [0.08, 0.07], closed: false, confidence: 0.9 },
    { bounds: [0.6, 0.2, 0.8, 0.4], centre: [0.7, 0.3], tilt: 2, eyes: [0.0, 0.01], closed: true, confidence: 0.8 },
  ]);
  render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={onSave} onLiquify={vi.fn()} onFindFaces={onFindFaces} subject="p/1" />);
  fireEvent.click(screen.getByRole('tab', { name: 'Faces' }));
  expect(screen.getByText('Every face.')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Find faces' }));
  const second = await screen.findByLabelText(/Face 2 - right/);
  expect(screen.getByText(/eyes closed/)).toBeTruthy();
  fireEvent.click(second);
  expect(onSave.mock.lastCall?.[0].faces).toEqual([[0.7, 0.3]]);
  expect(onSave.mock.lastCall?.[1]).toBe('Faces: chosen');
});

it('says so on the body and people tabs when the photograph has no face, and looks only once', async () => {
  const onFindFaces = vi.fn().mockResolvedValue([]);
  render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={vi.fn()} onLiquify={vi.fn()} onFindFaces={onFindFaces} subject="p/1" />);
  expect(onFindFaces).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('tab', { name: 'Body' }));
  expect(await screen.findByText(/No face was found in this photograph/)).toBeTruthy();
  fireEvent.click(screen.getByRole('tab', { name: 'People & scene' }));
  expect(screen.getByText(/No face was found in this photograph/)).toBeTruthy();
  expect(onFindFaces).toHaveBeenCalledTimes(1);
});

it('says nothing on the body tab when a face is found or the lookup fails', async () => {
  const face = { bounds: [0.1, 0.2, 0.3, 0.4], centre: [0.2, 0.3], tilt: 0, eyes: [0.08, 0.07], closed: false, confidence: 0.9 };
  for (const onFindFaces of [vi.fn().mockResolvedValue([face]), vi.fn().mockRejectedValue(new Error('unreadable'))]) {
    const { unmount } = render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={vi.fn()} onLiquify={vi.fn()} onFindFaces={onFindFaces} subject="p/1" initialTab="body" />);
    await waitFor(() => expect(onFindFaces).toHaveBeenCalledTimes(1));
    await act(async () => { await Promise.resolve(); });
    expect(screen.queryByText(/No face was found in this photograph/)).toBeNull();
    unmount();
  }
});

it('offers the two new backgrounds', () => {
  const onSave = vi.fn();
  render(<PortraitFinishPanel finish={NEUTRAL_FINISH} disabled={false} liquify={null} onSave={onSave} onLiquify={vi.fn()} />);
  fireEvent.click(screen.getByRole('tab', { name: 'Background' }));
  fireEvent.click(screen.getByRole('button', { name: 'Colour, keep shadows' }));
  expect(onSave.mock.lastCall?.[0].background.mode).toBe('keep_shadows');
  fireEvent.click(screen.getByRole('button', { name: 'Sky with clouds' }));
  expect(onSave.mock.lastCall?.[0].background.mode).toBe('cloud_sky');
});
