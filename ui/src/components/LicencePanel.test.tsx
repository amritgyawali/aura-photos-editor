import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { LicencePanel, licenceBadge } from './LicencePanel';
import { licence, type LicenceStatus } from '../ipc/client';

vi.mock('../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (e: Error) => ({ message: e.message }),
  licence: { status: vi.fn(), activate: vi.fn(), deactivate: vi.fn() },
}));

const ended: LicenceStatus = { state: 'trial_ended', mayExport: false, name: null, email: null, edition: null, expires: null, daysLeft: null, trialEnds: '2026-10-21',
  message: 'Your free trial has ended. Editing still works; enter a licence key to export finished photographs.' };
const licensed: LicenceStatus = { ...ended, state: 'licensed', mayExport: true, name: 'Asha Studio', email: 'asha@example.com', edition: 'pro', message: 'Licensed to Asha Studio.' };

beforeEach(() => { vi.clearAllMocks(); vi.mocked(licence.status).mockResolvedValue(ended); });

it('activates a pasted key and shows who it is licensed to', async () => {
  vi.mocked(licence.activate).mockResolvedValue(licensed);
  const onChange = vi.fn();
  render(<LicencePanel onChange={onChange} />);
  expect((await screen.findByRole('status')).textContent).toContain('trial has ended');
  fireEvent.change(screen.getByLabelText('Licence key'), { target: { value: 'AURA1.abc.def' } });
  fireEvent.click(screen.getByRole('button', { name: 'Activate' }));
  expect(await screen.findByText('Asha Studio')).toBeTruthy();
  expect(licence.activate).toHaveBeenCalledWith('AURA1.abc.def');
  expect(onChange).toHaveBeenLastCalledWith(licensed);
  expect(screen.getByRole('button', { name: /Remove from this computer/ })).toBeTruthy();
});

it('shows why a key was refused', async () => {
  vi.mocked(licence.activate).mockRejectedValue(new Error('That licence key does not check out.'));
  render(<LicencePanel />);
  await screen.findByRole('status');
  fireEvent.change(screen.getByLabelText('Licence key'), { target: { value: 'nope' } });
  fireEvent.click(screen.getByRole('button', { name: 'Activate' }));
  expect((await screen.findByRole('alert')).textContent).toContain('does not check out');
});

it('badges the trial and its end, and nothing once licensed', () => {
  expect(licenceBadge({ ...ended, state: 'trial', mayExport: true, daysLeft: 3 })).toBe('Trial · 3 days left');
  expect(licenceBadge(ended)).toContain('activate');
  expect(licenceBadge(licensed)).toBeNull();
});
