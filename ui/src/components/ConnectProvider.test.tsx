import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '../ipc/client';
import { ConnectProviders } from './ConnectProvider';

vi.mock('../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (e: Error) => ({ code: 'X', message: e.message }),
  api: { openProviderPage: vi.fn() },
}));

beforeEach(() => { vi.clearAllMocks(); });

afterEach(cleanup);

it('Connect Claude opens the Anthropic key page and selects the provider', async () => {
  vi.mocked(api.openProviderPage).mockResolvedValue({ url: 'https://console.anthropic.com/settings/keys', opened: true, message: 'Your browser is open on the key page.' });
  const onConnect = vi.fn();
  render(<ConnectProviders onConnect={onConnect} />);
  fireEvent.click(screen.getByRole('button', { name: 'Connect Claude' }));
  expect((await screen.findByRole('status')).textContent).toContain('console.anthropic.com');
  expect(api.openProviderPage).toHaveBeenCalledWith('anthropic');
  expect(onConnect).toHaveBeenCalledWith('anthropic');
});

it('Connect ChatGPT asks for the OpenAI page and shows the address when no browser opens', async () => {
  vi.mocked(api.openProviderPage).mockResolvedValue({ url: 'https://platform.openai.com/api-keys', opened: false, message: 'The browser could not be opened.' });
  render(<ConnectProviders />);
  fireEvent.click(screen.getByRole('button', { name: 'Connect ChatGPT' }));
  expect((await screen.findByRole('status')).textContent).toContain('platform.openai.com/api-keys');
  expect(api.openProviderPage).toHaveBeenCalledWith('openai');
});
