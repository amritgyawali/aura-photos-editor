import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { RetouchCanvas } from './RetouchCanvas';
import { freshRetouch } from '../../ipc/nativeRetouch';

function pointer(element: HTMLElement, type: string, x: number, y: number, pressure = .4) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.assign(event, { pointerId: 1, button: 0, clientX: x, clientY: y, pressure, pointerType: 'pen' });
  fireEvent(element, event);
}
function open(disabled = false) {
  const stroke = vi.fn(), source = vi.fn();
  render(<RetouchCanvas src="data:image/png;base64,AA==" width={200} height={100} compare={false} disabled={disabled}
    draft={freshRetouch()} mode="paint" radius={.05} opacity={.7} overlay sourceMode={false}
    onTarget={vi.fn()} onSource={source} onStroke={stroke} onNotice={vi.fn()}/>);
  const surface = screen.getByLabelText('Retouch image interaction');
  vi.spyOn(surface, 'getBoundingClientRect').mockImplementation(() => ({ left: 100, top: 50,
    width: Number.parseFloat(surface.style.width), height: Number.parseFloat(surface.style.height) } as DOMRect));
  return { surface, stroke };
}
it('maps pointer strokes through zoom and preserves pressure without saving on every move', () => {
  const { surface, stroke } = open();
  fireEvent.click(screen.getByRole('button', { name: 'Zoom in' }));
  pointer(surface, 'pointerdown', 250, 125, .2);
  pointer(surface, 'pointermove', 325, 125, .9);
  expect(stroke).not.toHaveBeenCalled();
  pointer(surface, 'pointerup', 325, 125);
  expect(stroke).toHaveBeenCalledWith({ erase: false, radius: .05, opacity: .7,
    points: [[.5, .5, .2], [.75, .5, .9]] });
});
it('cancels interrupted strokes and blocks painting while another edit is running', () => {
  const { surface, stroke } = open(true);
  pointer(surface, 'pointerdown', 200, 100); pointer(surface, 'pointerup', 220, 100);
  expect(stroke).not.toHaveBeenCalled();
});
it('discards pointer cancellation instead of saving a partial gesture', () => {
  const { surface, stroke } = open();
  pointer(surface, 'pointerdown', 200, 100); pointer(surface, 'pointermove', 220, 100);
  pointer(surface, 'pointercancel', 220, 100); pointer(surface, 'pointerup', 220, 100);
  expect(stroke).not.toHaveBeenCalled();
});
