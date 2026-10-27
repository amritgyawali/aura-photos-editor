import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { SavedRetouchStrength } from './SavedRetouchStrength';

it('keeps slider changes unsaved until applied and accepts zero strength', () => {
  const onApply = vi.fn();
  render(<SavedRetouchStrength index={2} amount={0.65} disabled={false} onApply={onApply}/>);
  fireEvent.change(screen.getByRole('slider', { name: 'Retouch 2 strength' }), { target: { value: '20' } });
  expect(onApply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply retouch 2 strength' }));
  expect(onApply).toHaveBeenLastCalledWith(0.2);
  fireEvent.change(screen.getByRole('slider', { name: 'Retouch 2 strength' }), { target: { value: '0' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply retouch 2 strength' }));
  expect(onApply).toHaveBeenLastCalledWith(0);
});

it('discards a draft and follows strength changes saved elsewhere', () => {
  const onApply = vi.fn();
  const view = render(<SavedRetouchStrength index={1} amount={0.4} disabled={false} onApply={onApply}/>);
  fireEvent.change(screen.getByRole('slider'), { target: { value: '90' } });
  fireEvent.click(screen.getByRole('button', { name: 'Discard retouch 1 strength' }));
  expect((screen.getByRole('slider') as HTMLInputElement).value).toBe('40');
  view.rerender(<SavedRetouchStrength index={1} amount={0.7} disabled={false} onApply={onApply}/>);
  expect((screen.getByRole('slider') as HTMLInputElement).value).toBe('70');
  expect(onApply).not.toHaveBeenCalled();
});
