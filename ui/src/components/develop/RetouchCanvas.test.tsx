import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { RetouchCanvas } from './RetouchCanvas';
import { freshRetouch } from '../../ipc/nativeRetouch';

function pointer(element: HTMLElement, type: string, x: number, y: number, pressure = .4) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.assign(event, { pointerId: 1, button: 0, clientX: x, clientY: y, pressure, pointerType: 'pen' });
  fireEvent(element, event);
}
function open(disabled = false, split = false, gradient = false, maskView = false) {
  const stroke = vi.fn(), source = vi.fn(), target = vi.fn(), onGradient = vi.fn();
  render(<RetouchCanvas src="data:image/png;base64,AA==" width={200} height={100} compare={false} disabled={disabled}
    beforeSrc="data:image/png;base64,AQ==" split={split}
    draft={freshRetouch()} mode={gradient?'gradient':'paint'} maskView={maskView} onGradient={onGradient} radius={.05} opacity={.7} overlay sourceMode={false}
    onTarget={target} onSource={source} onStroke={stroke} onNotice={vi.fn()}/>);
  const surface = screen.getByLabelText('Retouch image interaction');
  vi.spyOn(surface, 'getBoundingClientRect').mockImplementation(() => ({ left: 100, top: 50,
    width: Number.parseFloat(surface.style.width), height: Number.parseFloat(surface.style.height) } as DOMRect));
  return { surface, stroke, source, target, onGradient };
}
it('maps gradient drags through zoom and cancels interrupted gradients', () => {
  const {surface,onGradient,stroke} = open(false,false,true);
  fireEvent.click(screen.getByRole('button',{name:'Zoom in'}));
  pointer(surface,'pointerdown',175,125);
  pointer(surface,'pointermove',325,125);
  expect(onGradient).not.toHaveBeenCalled();
  pointer(surface,'pointerup',325,125);
  expect(onGradient).toHaveBeenCalledWith([.25,.5],[.75,.5]);
  pointer(surface,'pointerdown',175,125);
  pointer(surface,'pointercancel',325,125);
  expect(onGradient).toHaveBeenCalledTimes(1);
  expect(stroke).not.toHaveBeenCalled();
});
it('keeps the selection mask read-only during pointer gestures', () => {
  const {surface,onGradient,stroke,target,source} = open(false,false,true,true);
  pointer(surface,'pointerdown',150,100); pointer(surface,'pointerup',250,100);
  for(const callback of [onGradient,stroke,target,source]) expect(callback).not.toHaveBeenCalled();
  expect(screen.getByAltText('Selection mask')).toBeTruthy();
});
it('aligns both comparison images and exposes the complete before and after through the slider', () => {
  const { surface } = open(false, true);
  const before = screen.getByAltText('Before native retouch comparison');
  expect(before.parentElement).toBe(surface);
  expect(screen.getByAltText('Retouched photograph').parentElement).toBe(surface);
  const slider = screen.getByLabelText('Before/after split');
  fireEvent.change(slider, { target: { value: '100' } });
  expect(before.style.clipPath).toBe('inset(0 0% 0 0)');
  expect(slider.getAttribute('aria-valuetext')).toBe('100% before, 0% retouched');
  fireEvent.change(slider, { target: { value: '0' } });
  expect(before.style.clipPath).toBe('inset(0 100% 0 0)');
  fireEvent.click(screen.getByText('Center divider'));
  expect((slider as HTMLInputElement).value).toBe('50');
});
it('drags the divider through zoom without painting or changing the source', () => {
  const { surface, stroke, source, target } = open(false, true);
  fireEvent.click(screen.getByRole('button', { name: 'Zoom in' }));
  const divider = surface.querySelector<HTMLElement>('.retouch-compare-divider')!;
  pointer(divider, 'pointerdown', 250, 125);
  pointer(divider, 'pointermove', 325, 125);
  pointer(divider, 'pointerup', 325, 125);
  expect((screen.getByLabelText('Before/after split') as HTMLInputElement).value).toBe('75');
  pointer(surface, 'pointerdown', 200, 100);
  pointer(surface, 'pointermove', 220, 100);
  pointer(surface, 'pointerup', 220, 100);
  expect(stroke).not.toHaveBeenCalled();
  expect(source).not.toHaveBeenCalled();
  expect(target).not.toHaveBeenCalled();
});
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
