import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ProviderConnect } from './ProviderConnect';
import { api } from '../ipc/client';
vi.mock('../ipc/client', () => ({ api: {openAiProviderPage: vi.fn()}, asIpcError: (e: unknown) => ({message: e instanceof Error ? e.message : String(e)}) }));
afterEach(cleanup);
beforeEach(() => { vi.mocked(api.openAiProviderPage).mockReset(); });

it.each([['openai','OpenAI / ChatGPT'],['anthropic','Claude']])('opens %s by identity and explains the remaining connection steps', async (provider, label) => {
  vi.mocked(api.openAiProviderPage).mockResolvedValue('https://provider.example/keys');
  const opened = vi.fn();
  render(<ProviderConnect provider={provider} label={label} onOpened={opened}/>);
  fireEvent.click(screen.getByRole('button', {name:`Connect ${label} in browser`}));
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect(api.openAiProviderPage).toHaveBeenCalledWith(provider);
  expect(screen.getByRole('status').textContent).toContain('paste it, save, then check');
});

it('reports browser failure without marking the connection successful', async () => {
  let refuse: (reason: Error) => void = () => {};
  vi.mocked(api.openAiProviderPage).mockReturnValue(new Promise((_, reject) => { refuse = reject; }));
  const opened = vi.fn();
  const opening = vi.fn();
  render(<ProviderConnect provider="anthropic" label="Claude" onOpened={opened} onOpeningChange={opening}/>);
  fireEvent.click(screen.getByRole('button'));
  await act(async () => { refuse(new Error('No system browser')); });
  expect(screen.getByRole('alert').textContent).toBe('No system browser');
  expect(opened).not.toHaveBeenCalled();
  expect(screen.queryByRole('status')).toBeNull();
  expect(opening.mock.calls).toEqual([[true],[false]]);
});

it('allows only one browser launch while opening', async () => {
  vi.mocked(api.openAiProviderPage).mockReturnValue(new Promise(() => {}));
  render(<ProviderConnect provider="openai" label="OpenAI"/>);
  fireEvent.click(screen.getByRole('button'));
  fireEvent.click(screen.getByRole('button'));
  expect(api.openAiProviderPage).toHaveBeenCalledOnce();
  expect((screen.getByRole('button') as HTMLButtonElement).disabled).toBe(true);
});
