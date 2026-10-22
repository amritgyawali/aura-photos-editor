import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { MASK_LAB_TOOLS, MaskLabControls } from './MaskLabControls';
import type { LocalMask } from '../../ipc/localMasks';
const source: LocalMask = { id: 'one', name: 'Source', enabled: true, amount: 1, params: { exposure: 0.5 }, components: [{ mode: 'add', source: { type: 'linear', start: [0, 0], end: [1, 0] } }] };
function setup(disabled = false, second = true) {
  const onWorkflow = vi.fn();
  const view = render(<MaskLabControls masks={second ? [source, { ...source, id: 'two', name: 'Destination' }] : [source]} selected="one" disabled={disabled} onWorkflow={onWorkflow} />);
  return { onWorkflow, ...view };
}
it('exposes exactly 100 distinct working requests and queues nothing before explicit Apply', () => {
  const { onWorkflow } = setup();
  expect(new Set(MASK_LAB_TOOLS.map(item => item.action)).size).toBe(100);
  expect(screen.getByText('Selection laboratory · 100 workflows')).toBeTruthy();
  const frozen = JSON.stringify(source);
  for (const tool of MASK_LAB_TOOLS) {
    fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value: tool.action } });
    if (['thin_plate_coverage_warp', 'mean_value_cage_warp', 'triangulated_cage_warp'].includes(tool.action)) {
      while (screen.getAllByText('Remove vertex').length < 8) fireEvent.click(screen.getByText('Add vertex or seed'));
      const vertices = [[0.1, 0.1], [0.15, 0.1], [0.9, 0.1], [0.85, 0.15], [0.9, 0.9], [0.9, 0.85], [0.1, 0.9], [0.15, 0.9]];
      vertices.forEach((point, index) => point.forEach((value, axis) => fireEvent.change(screen.getByLabelText(`Vertex ${index + 1} ${axis === 0 ? 'X' : 'Y'}`), { target: { value: String(value) } })));
    }
    expect(screen.getByRole('note').textContent).toBe(tool.hint);
    expect(onWorkflow).toHaveBeenCalledTimes(MASK_LAB_TOOLS.indexOf(tool));
    fireEvent.click(screen.getByText('Apply laboratory workflow'));
    expect(onWorkflow).toHaveBeenLastCalledWith(expect.objectContaining({ action: tool.action, advanced: expect.objectContaining({ vertices: expect.any(Array), bounds: [0.2, 0.2, 0.8, 0.8] }) }));
    if (tool.controls.includes('target')) expect(onWorkflow).toHaveBeenLastCalledWith(expect.objectContaining({ otherMaskId: 'two' }));
    if (tool.controls.includes('point')) expect(onWorkflow).toHaveBeenLastCalledWith(expect.objectContaining({ point: [0.5, 0.5] }));
  }
  expect(JSON.stringify(source)).toBe(frozen);
}, 20_000);
it('authors normalized vertices, samples and limits in local draft state', () => {
  const { onWorkflow } = setup();
  fireEvent.change(screen.getByLabelText('Vertex 1 X'), { target: { value: '0.35' } });
  expect(onWorkflow).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Apply laboratory workflow'));
  expect(onWorkflow).toHaveBeenLastCalledWith(expect.objectContaining({ advanced: expect.objectContaining({ vertices: [[0.35, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]] }) }));
  fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value: 'sample_luminance' } });
  fireEvent.change(screen.getByLabelText('Laboratory sample X'), { target: { value: '0.25' } });
  fireEvent.change(screen.getByLabelText('Laboratory radius'), { target: { value: '8' } });
  fireEvent.change(screen.getByLabelText('Laboratory tolerance'), { target: { value: '0.05' } });
  fireEvent.click(screen.getByText('Apply laboratory workflow'));
  expect(onWorkflow).toHaveBeenLastCalledWith(expect.objectContaining({ point: [0.25, 0.5], advanced: expect.objectContaining({ radius: 8, tolerance: 0.05 }) }));
});
it('refuses missing destinations, invalid geometry counts and excessive guided radius', () => {
  setup(false, false);
  const choose = (value: string) => fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value } });
  choose('append_stack');
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  choose('polygon_marquee');
  fireEvent.click(screen.getByLabelText('Remove vertex 4'));
  fireEvent.click(screen.getByLabelText('Remove vertex 3'));
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  choose('perspective_selection');
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  choose('guided_smooth');
  fireEvent.change(screen.getByLabelText('Laboratory radius'), { target: { value: '9' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
});
it('disables all workflow mutations during native processing', () => {
  setup(true);
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  expect(screen.getByLabelText('Selection laboratory workflow').matches(':disabled')).toBe(true);
  expect(screen.getByLabelText('Vertex 1 X').matches(':disabled')).toBe(true);
});
it('validates ordered shape bounds, ranges and area criteria', () => {
  setup();
  const choose = (value: string) => fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value } });
  choose('rounded_marquee');
  fireEvent.change(screen.getByLabelText('Laboratory bound left'), { target: { value: '0.9' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  choose('saturation_range');
  fireEvent.change(screen.getByLabelText('Laboratory range low'), { target: { value: '0.9' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  choose('filter_regions');
  fireEvent.change(screen.getByLabelText('Laboratory minArea'), { target: { value: '100' } });
  fireEvent.change(screen.getByLabelText('Laboratory maxArea'), { target: { value: '10' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
});
it('rejects incomplete Bezier and seed fields and enforces source-analysis patch limits', () => {
  setup();
  const choose = (value: string) => fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value } });
  choose('bezier_ribbon');
  fireEvent.click(screen.getByLabelText('Remove vertex 4'));
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  choose('seed_field');
  fireEvent.click(screen.getByLabelText('Remove vertex 3'));
  fireEvent.click(screen.getByLabelText('Remove vertex 2'));
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  choose('local_entropy');
  fireEvent.change(screen.getByLabelText('Laboratory radius'), { target: { value: '9' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
});

it('does not block a workflow because an unrelated hidden patch radius was previously excessive', () => {
  const { onWorkflow } = setup();
  fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value: 'guided_smooth' } });
  fireEvent.change(screen.getByLabelText('Laboratory radius'), { target: { value: '9' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value: 'directional_ridge' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(false);
  fireEvent.click(screen.getByText('Apply laboratory workflow'));
  expect(onWorkflow).toHaveBeenLastCalledWith(expect.objectContaining({ action: 'directional_ridge', advanced: expect.objectContaining({ radius: 4 }) }));
});
it('validates template budgets and paired landmark counts before applying', () => {
  setup();
  fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value: 'template_patch_correlation' } });
  fireEvent.change(screen.getByLabelText('Laboratory radius'), { target: { value: '5' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value: 'thin_plate_coverage_warp' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  fireEvent.click(screen.getByText('Add vertex or seed'));
  fireEvent.click(screen.getByText('Add vertex or seed'));
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  fireEvent.change(screen.getByLabelText('Vertex 6 Y'), { target: { value: '0.7' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(false);
  fireEvent.change(screen.getByLabelText('Selection laboratory workflow'), { target: { value: 'mean_value_cage_warp' } });
  expect(screen.getByText('Apply laboratory workflow').matches(':disabled')).toBe(true);
  expect(screen.getByText(/Selection laboratory.*100 workflows/)).toBeTruthy();
});
