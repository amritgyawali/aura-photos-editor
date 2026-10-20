import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { badgesOf, EffectsLibrary, LIBRARY_GROUPS } from './EffectsLibrary';
import { RETOUCH_TOOLS } from '../../ipc/nativeRetouch';
import { EFFECT_SHORTCUTS } from '../../ipc/retouchEffects';
import { RECENT_LIMIT, useEffectRequest } from '../../state/effectRequest';

beforeEach(() => {
  useEffectRequest.setState({ request: null, sequence: 0, recent: [], favourites: [] });
});

it('lists every retouch tool and effect once, in its group', () => {
  render(<EffectsLibrary photoName="DSC_0001.NEF" onOpen={() => undefined} />);
  expect(screen.getByText(`All ${RETOUCH_TOOLS.length} tools`)).toBeTruthy();
  const cards = screen.getAllByRole('button', { name: 'Use on this photo' });
  expect(cards).toHaveLength(RETOUCH_TOOLS.length);
  for (const group of LIBRARY_GROUPS) expect(screen.getByRole('region', { name: group })).toBeTruthy();
});

it('search narrows by name, group and description together', () => {
  render(<EffectsLibrary photoName="a.jpg" onOpen={() => undefined} />);
  fireEvent.change(screen.getByLabelText('Search tools'), { target: { value: 'jewellery' } });
  expect(screen.getByText('Sparkle')).toBeTruthy();
  expect(screen.queryByText('Pixel sort')).toBeNull();
  // Quick jobs are a starting page, not a search result.
  expect(screen.queryByRole('region', { name: 'Quick jobs' })).toBeNull();
  fireEvent.change(screen.getByLabelText('Search tools'), { target: { value: 'nothing matches this' } });
  expect(screen.getByText('No tool matches that search.')).toBeTruthy();
});

it('a group chip shows only that group', () => {
  render(<EffectsLibrary photoName="a.jpg" onOpen={() => undefined} />);
  fireEvent.click(screen.getByRole('button', { name: 'Liquify' }));
  const expected = RETOUCH_TOOLS.filter(t => t[2] === 'Liquify').length;
  expect(screen.getAllByRole('button', { name: 'Use on this photo' })).toHaveLength(expected);
});

it('using a tool asks Retouch for it, remembers it and opens the photo', () => {
  const onOpen = vi.fn();
  render(<EffectsLibrary photoName="a.jpg" onOpen={onOpen} />);
  const card = screen.getByRole('heading', { name: 'Fairy lights' }).closest('li') as HTMLElement;
  fireEvent.click(within(card).getByRole('button', { name: 'Use on this photo' }));
  expect(onOpen).toHaveBeenCalledOnce();
  const state = useEffectRequest.getState();
  expect(state.request).toEqual({ tool: 'fairy_lights', patch: {}, sequence: 1 });
  expect(state.recent).toEqual(['fairy_lights']);
  fireEvent.click(screen.getByRole('button', { name: 'Recently used' }));
  expect(screen.getAllByRole('button', { name: 'Use on this photo' })).toHaveLength(1);
});

it('a quick job carries its starting point', () => {
  render(<EffectsLibrary photoName="a.jpg" onOpen={() => undefined} />);
  const shortcut = EFFECT_SHORTCUTS.find(s => s.tool === 'sparkle');
  expect(shortcut).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: new RegExp(shortcut?.label ?? '') }));
  expect(useEffectRequest.getState().request).toMatchObject({ tool: 'sparkle', patch: shortcut?.patch });
});

it('without a photograph the library is for browsing and asks for nothing', () => {
  const onOpen = vi.fn();
  render(<EffectsLibrary photoName={null} onOpen={onOpen} />);
  expect(screen.getByRole('status').textContent).toMatch(/choose a photo/i);
  const buttons = screen.getAllByRole('button', { name: 'Use on this photo' }) as HTMLButtonElement[];
  expect(buttons.every(b => b.disabled)).toBe(true);
  expect(onOpen).not.toHaveBeenCalled();
  expect(useEffectRequest.getState().request).toBeNull();
});

it('favourites are toggled from a card and listed in their own view', () => {
  render(<EffectsLibrary photoName="a.jpg" onOpen={() => undefined} />);
  fireEvent.click(screen.getByRole('button', { name: 'Add Emboss to favourites' }));
  expect(useEffectRequest.getState().favourites).toEqual(['emboss']);
  fireEvent.click(screen.getByRole('button', { name: 'Favourites (1)' }));
  expect(screen.getAllByRole('button', { name: 'Use on this photo' })).toHaveLength(1);
  fireEvent.click(screen.getByRole('button', { name: 'Remove Emboss from favourites' }));
  expect(screen.getByText(/No favourites yet/)).toBeTruthy();
});

it('recently used is bounded and most recent first', () => {
  const { ask } = useEffectRequest.getState();
  RETOUCH_TOOLS.slice(0, RECENT_LIMIT + 3).forEach(([tool]) => ask(tool));
  ask(RETOUCH_TOOLS[0][0]);
  const { recent } = useEffectRequest.getState();
  expect(recent).toHaveLength(RECENT_LIMIT);
  expect(recent[0]).toBe(RETOUCH_TOOLS[0][0]);
  expect(new Set(recent).size).toBe(recent.length);
});

it('consuming a request clears only that request', () => {
  const { ask, consume } = useEffectRequest.getState();
  ask('sparkle');
  ask('emboss');
  consume(1);
  expect(useEffectRequest.getState().request?.tool).toBe('emboss');
  consume(2);
  expect(useEffectRequest.getState().request).toBeNull();
});

it('a card says what a tool needs before it can run', () => {
  expect(badgesOf('colour_splash')).toContain('Pick a point');
  expect(badgesOf('god_rays')).toContain('Optional point');
  expect(badgesOf('skin_smooth')).toContain('Sample skin');
  expect(badgesOf('ripple')).toContain('Moves pixels');
  expect(badgesOf('fairy_lights')).toEqual(['Choose a colour', '3 sliders']);
  expect(badgesOf('heal')).toEqual([]);
});
