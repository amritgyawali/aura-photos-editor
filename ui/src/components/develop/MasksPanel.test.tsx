import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { MasksPanel, type MasksPanelProps } from './MasksPanel';
import { describe as describeComponent, masksOf, type LocalMask } from '../../ipc/localMasks';
import type { RecipeDto } from '../../ipc/types';

const sky: LocalMask = {
  id: 'm1', name: 'Sky 1', enabled: true, amount: 1, params: {},
  components: [{ mode: 'add', source: { type: 'matte', matte: 'sky-1', what: 'sky' } }],
};

function panel(overrides: Partial<MasksPanelProps> = {}) {
  const props: MasksPanelProps = {
    masks: [], selected: null, disabled: false, message: null, tool: null,
    brush: { size: 0.04, feather: 0.5, flow: 1, erase: false }, overlay: true,
    onSelect: vi.fn(), onCreate: vi.fn(), onSave: vi.fn(), onTool: vi.fn(), onBrush: vi.fn(), onOverlay: vi.fn(),
    ...overrides,
  };
  render(<MasksPanel {...props} />);
  return props;
}

it('creates AI selections by name and starts drawing tools', () => {
  const props = panel();
  fireEvent.click(screen.getByText('Subject'));
  expect(props.onCreate).toHaveBeenCalledWith({ what: 'subject' });
  fireEvent.change(screen.getByLabelText('People and face'), { target: { value: 'hair' } });
  expect(props.onCreate).toHaveBeenCalledWith({ what: 'hair' });
  fireEvent.click(screen.getByText('Radial gradient'));
  expect(props.onTool).toHaveBeenCalledWith({ kind: 'radial', into: null, mode: 'add', invert: false });
});

it('a mask slider commits only on release, and zero removes the setting', () => {
  const props = panel({ masks: [sky], selected: 'm1' });
  const exposure = screen.getByLabelText('Mask Exposure');
  fireEvent.change(exposure, { target: { value: '-0.5' } });
  expect(props.onSave).not.toHaveBeenCalled();
  fireEvent.pointerUp(exposure);
  expect(props.onSave).toHaveBeenCalledWith([{ ...sky, params: { exposure: -0.5 } }], 'Mask exposure');
  fireEvent.change(exposure, { target: { value: '0' } });
  fireEvent.pointerUp(exposure);
  expect(props.onSave).toHaveBeenLastCalledWith([{ ...sky, params: {} }], 'Mask exposure');
});

it('combines, inverts and deletes', () => {
  const props = panel({ masks: [sky], selected: 'm1' });
  fireEvent.change(screen.getByLabelText('How to combine'), { target: { value: 'subtract' } });
  fireEvent.change(screen.getByLabelText('Selection to combine'), { target: { value: 'subject' } });
  fireEvent.click(screen.getByText('Apply'));
  expect(props.onCreate).toHaveBeenCalledWith({ what: 'subject', into: 'm1', mode: 'subtract', invert: false });
  fireEvent.click(screen.getByLabelText('Invert + Sky'));
  expect(props.onSave).toHaveBeenCalledWith([{ ...sky, components: [{ ...sky.components[0], invert: true }] }], 'Invert mask component');
  fireEvent.click(screen.getByLabelText('Delete Sky 1'));
  expect(props.onSave).toHaveBeenLastCalledWith([], 'Delete mask');
  expect(props.onSelect).toHaveBeenCalledWith(null);
});

it('reads masks from the recipe body and describes components', () => {
  const recipe = { body: JSON.stringify({ studio_masks_v1: [sky] }) } as RecipeDto;
  expect(masksOf(recipe)).toEqual([sky]);
  expect(masksOf({ body: '{' } as RecipeDto)).toEqual([]);
  expect(describeComponent({ mode: 'subtract', invert: true, source: { type: 'matte', matte: 'x', what: 'subject' } })).toBe('− not Subject');
});

it('exposes twelve native selection actions and keeps the other mask explicit', () => {
  const props = panel({ masks: [sky, { ...sky, id: 'm2', name: 'Other selection' }], selected: 'm1', onWorkflow: vi.fn() });
  const cases = [
    ['Duplicate whole mask', { action: 'duplicate' }], ['Invert whole selection', { action: 'invert' }],
    ['Freeze composed selection', { action: 'freeze' }],
    ['Union with saved mask', { action: 'union', otherMaskId: 'm2' }],
    ['Intersect with saved mask', { action: 'intersection', otherMaskId: 'm2' }],
    ['Subtract saved mask', { action: 'subtraction', otherMaskId: 'm2' }],
    ['Feather saved selection', { action: 'feather', value: 3 }], ['Expand selection', { action: 'expand', value: 3 }],
    ['Contract selection', { action: 'contract', value: 3 }], ['Refine edge contrast', { action: 'edge_contrast', value: 25 }],
    ['Remove small selection islands', { action: 'remove_islands', value: 16 }], ['Fill small selection holes', { action: 'fill_holes', value: 16 }],
  ] as const;
  for (const [label, request] of cases) {
    fireEvent.click(screen.getByText(label));
    expect(props.onWorkflow).toHaveBeenLastCalledWith(request);
  }
  expect(props.onSave).not.toHaveBeenCalled();
});

it('disables selection operations while saving', () => {
  panel({ masks: [sky], selected: 'm1', disabled: true, onWorkflow: vi.fn() });
  expect(screen.getByText('Invert whole selection').matches(':disabled')).toBe(true);
  expect(screen.getByLabelText('Second saved mask').matches(':disabled')).toBe(true);
});

it('does not offer a refinement with zero radius or a second mask that no longer exists', () => {
  panel({ masks: [sky], selected: 'm1', onWorkflow: vi.fn() });
  fireEvent.change(screen.getByLabelText('Selection refinement radius'), { target: { value: '0' } });
  expect(screen.getByText('Expand selection').matches(':disabled')).toBe(true);
  expect(screen.getByText('Union with saved mask').matches(':disabled')).toBe(true);
});
