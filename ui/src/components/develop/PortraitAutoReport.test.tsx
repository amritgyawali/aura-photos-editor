import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { PortraitAutoReport, portraitMessage } from './PortraitAutoReport';
import type { RecipeDto } from '../../ipc/types';

const recipe = (report: unknown) => ({ body: JSON.stringify({ studio_portrait_auto_v1: report }) }) as RecipeDto;
describe('automatic face decisions', () => {
  it('shows measured strengths and explains detection confidence', () => {
    render(<PortraitAutoReport recipe={recipe({ message: 'Retouched two faces.', assessments: [
      { face: 1, status: 'retouched', confidence: .95, reason: 'Measured skin variation.', strengths: [.12,.09,.14] },
      { face: 2, status: 'skipped', confidence: .87, reason: 'Too small to sample.', strengths: [0,0,0] },
    ] })}/>);
    expect(screen.getByText('Automatic decisions by face (2)')).toBeTruthy();
    expect(screen.getByText('Texture 12% · Tone 9% · Light 14%')).toBeTruthy();
    expect(screen.getByText('Too small to sample.')).toBeTruthy();
    expect(screen.getByText(/not the quality of the edit/)).toBeTruthy();
  });
  it('accepts legacy reports and ignores malformed decisions', () => {
    expect(portraitMessage(recipe({message:'Legacy pass.'}))).toBe('Legacy pass.');
    render(<PortraitAutoReport recipe={recipe({message:'Legacy pass.',assessments:[null,{face:1}]})}/>);
    expect(screen.queryByText(/Automatic decisions by face/)).toBeNull();
  });
  it('tolerates a missing or damaged recipe', () => {
    expect(portraitMessage(null)).toBeNull();
    expect(portraitMessage({body:'invalid'} as RecipeDto)).toBeNull();
  });
});
