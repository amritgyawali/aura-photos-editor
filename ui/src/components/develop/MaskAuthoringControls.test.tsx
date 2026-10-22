import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { LocalMask, MaskSource } from '../../ipc/localMasks';
import { MaskWorkflowControls } from './MaskWorkflowControls';

const sources: MaskSource[] = [
  { type: 'linear', start: [0.1, 0.1], end: [0.8, 0.9] },
  { type: 'radial', centre: [0.4, 0.5], radii: [0.1, 0.2], angle: 12, feather: 0.3 },
  { type: 'brush', strokes: [{ erase: false, radius: 0.02, opacity: 0.6, points: [[0.2, 0.2, 0.8], [0.4, 0.4, 0.8]] }], feather: 0.3 },
  { type: 'luminance', range: { low: -2, high: 2, softness: 0.5 } },
];
const mask: LocalMask = { id: 'one', name: 'Authored', enabled: true, amount: 0.4, params: { exposure: 0.5 }, components: sources.map(source => ({ mode: 'add', source })) };
function setup(disabled = false) {
  const onWorkflow = vi.fn();
  render(<MaskWorkflowControls masks={[mask, { ...mask, id: 'two', name: 'Destination' }]} selected="one" disabled={disabled} onWorkflow={onWorkflow} />);
  return onWorkflow;
}
it.each([
  [0, 'Gradient start X', '0.3', 'edit_linear', { ...sources[0], start: [0.3, 0.1] }],
  [1, 'Ellipse angle', '45', 'edit_radial', { ...sources[1], angle: 45 }],
  [2, 'Authored point pressure', '0.4', 'edit_brush', { type: 'brush', strokes: [{ erase: false, radius: 0.02, opacity: 0.6, points: [[0.2, 0.2, 0.4], [0.4, 0.4, 0.8]] }], feather: 0.3 }],
  [3, 'Luminance low in stops', '-3', 'edit_luminance', { type: 'luminance', range: { low: -3, high: 2, softness: 0.5 } }],
] as const)('edits authored source %i only when applied', (index, label, value, action, source) => {
  const onWorkflow = setup();
  const before = JSON.stringify(mask);
  fireEvent.change(screen.getByLabelText('Authored component'), { target: { value: String(index) } });
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
  expect(onWorkflow).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Apply component geometry'));
  expect(onWorkflow).toHaveBeenCalledWith({ action, source, componentIndex: index });
  expect(JSON.stringify(mask)).toBe(before);
});
it('exposes composition, ordering, extraction and component transfer with explicit destinations', () => {
  const onWorkflow = setup();
  fireEvent.change(screen.getByLabelText('Authored component'), { target: { value: '1' } });
  fireEvent.change(screen.getByLabelText('Component combination'), { target: { value: 'subtract' } });
  fireEvent.click(screen.getByText('Apply combination mode'));
  expect(onWorkflow).toHaveBeenLastCalledWith({ action: 'component_mode', componentIndex: 1, mode: 'subtract' });
  fireEvent.change(screen.getByLabelText('Component destination position'), { target: { value: '4' } });
  fireEvent.click(screen.getByText('Reorder component'));
  expect(onWorkflow).toHaveBeenLastCalledWith({ action: 'reorder_component', componentIndex: 1, targetIndex: 3 });
  fireEvent.click(screen.getByText('Extract independent component mask'));
  expect(onWorkflow).toHaveBeenLastCalledWith({ action: 'extract_component', componentIndex: 1 });
  fireEvent.click(screen.getByText('Copy component to destination'));
  expect(onWorkflow).toHaveBeenLastCalledWith({ action: 'copy_component', componentIndex: 1, otherMaskId: 'two' });
});
it('exposes seven independently implemented composed-selection operations', () => {
  const onWorkflow = setup();
  for (const [label, request] of [
    ['Select boundary band', { action: 'border_selection', value: 3 }],
    ['Median speckle cleanup (radius up to 4)', { action: 'median_cleanup', value: 3 }],
    ['Keep largest connected selection', { action: 'keep_largest' }],
    ['Create convex hull selection', { action: 'convex_hull' }],
    ['Symmetric difference with saved mask', { action: 'exclusive_union', otherMaskId: 'two' }],
  ] as const) {
    fireEvent.click(screen.getByText(label));
    expect(onWorkflow).toHaveBeenLastCalledWith(request);
  }
  fireEvent.change(screen.getByLabelText('Selection seed X'), { target: { value: '0.3' } });
  fireEvent.click(screen.getByText('Isolate region at seed'));
  expect(onWorkflow).toHaveBeenLastCalledWith({ action: 'keep_connected', point: [0.3, 0.5] });
  fireEvent.change(screen.getByLabelText('Selection offset X'), { target: { value: '0.2' } });
  fireEvent.change(screen.getByLabelText('Selection scale Y'), { target: { value: '1.5' } });
  fireEvent.change(screen.getByLabelText('Selection rotation'), { target: { value: '30' } });
  fireEvent.click(screen.getByText('Apply selection placement'));
  expect(onWorkflow).toHaveBeenLastCalledWith({ action: 'transform_selection', transform: { offset: [0.2, 0], scale: [1, 1.5], angle: 30 } });
});
it('does not queue authored edits during a save or submit inverted luminance endpoints', () => {
  setup(true);
  expect(screen.getByText('Apply component geometry').matches(':disabled')).toBe(true);
  expect(screen.getByText('Apply selection placement').matches(':disabled')).toBe(true);
});
it('requires ordered luminance endpoints', () => {
  setup();
  fireEvent.change(screen.getByLabelText('Authored component'), { target: { value: '3' } });
  fireEvent.change(screen.getByLabelText('Luminance low in stops'), { target: { value: '4' } });
  expect(screen.getByText('Apply component geometry').matches(':disabled')).toBe(true);
});
