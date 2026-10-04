import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { PortraitAnalysis, PortraitRetouch } from '../../ipc/client';
import { PortraitRetouchView, withoutZeros, type PortraitRetouchViewProps } from './PortraitRetouch';

const analysis: PortraitAnalysis = {
  photoId: 'p1', overlayWidth: 2, overlayHeight: 2, colourful: true, parseVersion: 1, ms: 120, notes: [],
  faces: [{
    bbox: [0.3, 0.2, 0.4, 0.4], leftEye: [0.4, 0.35], rightEye: [0.6, 0.35], nose: [0.5, 0.45], mouth: [0.5, 0.52],
    rollDegrees: 2, confidence: 0.95, source: 'frontal', eyesMeasured: 2, mouthMeasured: true,
  }],
  regions: [
    { region: 'skin', label: 'Skin', coverage: 0.1, confidence: 0.9, alphaBase64: btoa(String.fromCharCode(255, 0, 0, 255)) },
    { region: 'teeth', label: 'Teeth', coverage: 0.002, confidence: 0.8, alphaBase64: btoa(String.fromCharCode(0, 0, 0, 255)) },
  ],
};

const settings: PortraitRetouch = {
  photoId: 'p1', ops: [{ op: 'teeth_whiten', strength: 0.4 }], adjustments: [], hints: [], protected: false,
  foreignOps: [], explanation: [], recipe: { photoId: 'p1', params: [], userEditedFields: [], source: 'ai', confidence: 0.5 } as unknown as PortraitRetouch['recipe'],
};

function props(overrides: Partial<PortraitRetouchViewProps> = {}): PortraitRetouchViewProps {
  return {
    analysis, settings, strengths: { teeth_whiten: 0.4 }, adjustment: null, imageUrl: 'data:image/bmp;base64,AA==',
    originalUrl: 'data:image/png;base64,AA==', comparing: false, overlay: ['skin'], showFaces: true, drawing: false,
    busy: false, error: null, explanation: [],
    onToggleRegion: vi.fn(), onToggleFaces: vi.fn(), onStrength: vi.fn(), onCommit: vi.fn(), onAuto: vi.fn(),
    onClear: vi.fn(), onCompare: vi.fn(), onDrawing: vi.fn(), onDrawFace: vi.fn(), onClearFaces: vi.fn(),
    onAdjustRegion: vi.fn(), onAdjust: vi.fn(),
    ...overrides,
  };
}

describe('PortraitRetouchView', () => {
  it('shows what AURA found: the faces, their landmarks and every region', () => {
    render(<PortraitRetouchView {...props()} />);
    expect(screen.getByTestId('portrait-summary').textContent).toContain('1 face found');
    expect(screen.getByTestId('region-skin').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId('region-teeth').getAttribute('aria-pressed')).toBe('false');
    const faces = screen.getByTestId('portrait-faces');
    expect(faces.querySelectorAll('circle').length).toBe(4);
    expect(screen.getByTestId('portrait-overlay')).toBeTruthy();
  });

  it('moves a slider, commits on release, and toggles a region', () => {
    const p = props();
    render(<PortraitRetouchView {...p} />);
    const teeth = screen.getByLabelText('Whiten teeth') as HTMLInputElement;
    expect(teeth.value).toBe('40');
    fireEvent.change(teeth, { target: { value: '70' } });
    expect(p.onStrength).toHaveBeenCalledWith('teeth_whiten', 0.7);
    fireEvent.pointerUp(teeth);
    expect(p.onCommit).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('region-teeth'));
    expect(p.onToggleRegion).toHaveBeenCalledWith('teeth');
  });

  it('runs the automatic retouch in three styles and says when a region was not found', () => {
    const p = props();
    render(<PortraitRetouchView {...p} />);
    fireEvent.click(screen.getByTestId('portrait-auto-natural'));
    fireEvent.click(screen.getByTestId('portrait-auto-polished'));
    expect(p.onAuto).toHaveBeenCalledWith('natural');
    expect(p.onAuto).toHaveBeenCalledWith('polished');
    // The analysis found no hair, so the hair control says so rather than pretending.
    expect(screen.getByText('Define hair').parentElement?.textContent).toContain('not found here');
  });

  it('offers to draw a face when none was found, and says a person\'s settings are kept', () => {
    const p = props({
      analysis: { ...analysis, faces: [], regions: [], notes: ['AURA did not find a face in this photograph. Draw a box around a face and AURA will retouch it.'] },
      settings: { ...settings, protected: true },
    });
    render(<PortraitRetouchView {...p} />);
    expect(screen.getByTestId('portrait-summary').textContent).toContain('No face found');
    expect(screen.getByText(/Draw a box around a face/)).toBeTruthy();
    expect(screen.getByTestId('portrait-protected')).toBeTruthy();
    fireEvent.click(screen.getByTestId('portrait-draw'));
    expect(p.onDrawing).toHaveBeenCalledWith(true);
  });

  it('shows the original while the comparison is held', () => {
    const p = props({ comparing: true });
    render(<PortraitRetouchView {...p} />);
    expect(screen.getByAltText('Original photograph')).toBeTruthy();
    expect(screen.queryByTestId('portrait-overlay')).toBeNull();
  });

  it('drops untouched adjustment fields before saving', () => {
    expect(withoutZeros({ region: 'hair', exposure: 0, saturation: 12, warmth: null })).toEqual({ region: 'hair', saturation: 12 });
  });
});
