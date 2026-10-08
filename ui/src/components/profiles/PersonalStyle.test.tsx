import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { PersonalStyle } from './PersonalStyle';
import type { LearnedStyle } from './profileSelection';

vi.mock('../../ipc/client', () => ({ asIpcError: (e: Error) => ({ message: e.message }) }));

const style: LearnedStyle = {
  profile: {
    id: 'personal-warm', name: 'Warm', category: 'Personal', tagline: '', description: '', bestFor: [], technique: [],
    origin: 'personal', sources: [{ title: '40 photographs', url: '' }], evidence: null, swatch: ['#ffffff'], adjust: {},
  },
  photos: 40, edited: 52, findings: ['Highlights -60 (90 % of photographs within a few points of it).'],
};

it('learns from the catalogue the photographer picks and reports what it found', async () => {
  const pick = vi.fn().mockResolvedValue('D:/lr/catalog.lrcat');
  const learn = vi.fn().mockResolvedValue(style);
  const onLearned = vi.fn();
  render(<PersonalStyle disabled={false} pick={pick} learn={learn} onLearned={onLearned} />);
  fireEvent.change(screen.getByLabelText('Style name'), { target: { value: 'Warm' } });
  fireEvent.click(screen.getByRole('button', { name: /Learn from Lightroom/ }));
  expect(await screen.findByText(/from 40 of 52 edited photographs/)).toBeTruthy();
  expect(learn).toHaveBeenCalledWith('D:/lr/catalog.lrcat', 'Warm');
  expect(onLearned).toHaveBeenCalledWith(style);
  expect(screen.getByText(/Highlights -60/)).toBeTruthy();
});

it('does nothing when the picker is cancelled, and shows a refusal in words', async () => {
  const learn = vi.fn().mockRejectedValue(new Error('Only 3 edited photographs in that catalogue.'));
  const { rerender } = render(<PersonalStyle disabled={false} pick={vi.fn().mockResolvedValue(null)} learn={learn} onLearned={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: /Learn from Lightroom/ }));
  await Promise.resolve();
  expect(learn).not.toHaveBeenCalled();
  rerender(<PersonalStyle disabled={false} pick={vi.fn().mockResolvedValue('x.lrcat')} learn={learn} onLearned={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: /Learn from Lightroom/ }));
  expect((await screen.findByRole('alert')).textContent).toContain('Only 3 edited photographs');
});
