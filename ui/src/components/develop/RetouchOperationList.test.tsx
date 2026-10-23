import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { freshRetouch } from '../../ipc/nativeRetouch';
import { RETOUCH_PAGE_SIZE, RetouchOperationList } from './RetouchOperationList';

const edits = Array.from({ length: 513 }, (_, index) => ({ ...freshRetouch(), id: `step-${index}`, enabled: index % 2 === 0 }));
const props = () => ({ edits, selected: null, disabled: false, automaticLabel: () => '', onSelect: vi.fn(), onUpdate: vi.fn(), onAction: vi.fn() });

it('bounds mounted rows and reaches later operations using their stable IDs', () => {
  const input = props();
  render(<RetouchOperationList {...input} />);
  expect(screen.getAllByRole('listitem')).toHaveLength(RETOUCH_PAGE_SIZE);
  fireEvent.click(screen.getByRole('button', { name: 'Next operations' }));
  expect(screen.getByRole('status').textContent).toContain('Showing 25–48 of 513');
  fireEvent.click(screen.getByLabelText('Remove retouch 25'));
  expect(input.onAction).toHaveBeenCalledWith('remove', 'step-24');
  fireEvent.change(screen.getByLabelText('Find operation'), { target: { value: 'step-512' } });
  expect(screen.getAllByRole('listitem')).toHaveLength(1);
  fireEvent.click(screen.getByLabelText('Move retouch 513 earlier'));
  expect(input.onAction).toHaveBeenLastCalledWith('earlier', 'step-512');
  expect((screen.getByLabelText('Move retouch 513 later') as HTMLButtonElement).disabled).toBe(true);
});

it('filters disabled steps, recovers from an empty search, and locates the selected step', () => {
  render(<RetouchOperationList {...props()} selected="step-512" />);
  fireEvent.click(screen.getByText('Show selected operation'));
  expect(screen.getByLabelText('Remove retouch 513')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Operation status'), { target: { value: 'disabled' } });
  expect(screen.queryByLabelText('Remove retouch 1')).toBeNull();
  expect(screen.getByLabelText('Remove retouch 2')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Find operation'), { target: { value: 'no match' } });
  expect(screen.getByText('No operations match your search.')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Find operation'), { target: { value: '' } });
  expect(screen.getAllByRole('listitem')).toHaveLength(RETOUCH_PAGE_SIZE);
});

it('keeps remaining operations reachable after the last page is removed', () => {
  const input = props();
  const view = render(<RetouchOperationList {...input} selected="step-512" />);
  fireEvent.click(screen.getByText('Show selected operation'));
  view.rerender(<RetouchOperationList {...input} edits={edits.slice(0, 24)} />);
  expect(screen.getAllByRole('listitem')).toHaveLength(24);
  expect(screen.getByLabelText('Remove retouch 1')).toBeTruthy();
});
