import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import type {
  MicroMatrixDto,
  MicroOpDto,
  MicroPlanDto,
  MicroStatusDto,
} from '../../ipc/types';

import { MicroRetouchPanel } from './MicroRetouchPanel';

const OPERATORS = ['flyaway', 'teeth', 'eyes', 'clothing', 'glare'];
const CLOTHING = ['lint', 'thread', 'stain', 'strap', 'crease'];
const FAMILIES = ['hair', 'teeth', 'eyes'];

function op(overrides: Partial<MicroOpDto> = {}): MicroOpDto {
  return {
    kind: 'flyaway',
    strength: 0.4,
    region: { x: 0.32, y: 0.05, w: 0.004, h: 0.08 },
    identityId: null,
    lumaEv: 0,
    yellowReduce: 0,
    sclera: 0,
    irisClarity: 0,
    clothingKind: null,
    method: null,
    borrowedFrom: null,
    alignment: 0,
    ...overrides,
  };
}

function plan(overrides: Partial<MicroPlanDto> = {}): MicroPlanDto {
  return {
    photoId: 'pht_1',
    ops: [op()],
    naturalness: {
      catchlightRatio: 1.0,
      hairEnergyRatio: 0.97,
      teethExcursion: 0,
      measuredOn: 4096,
      resolves: 0,
      withdrawn: [false, false, false],
      families: FAMILIES,
    },
    allowed: [true, true, true, true, true],
    operators: OPERATORS,
    reasons: [
      { code: 'micro_flyaway_calmed', text: 'A stray hair was calmed', weight: 0.1, doubt: false },
      {
        code: 'micro_background_busy',
        text: 'The background behind the hair is too busy to work against',
        weight: 0,
        doubt: true,
      },
    ],
    confidence: 0.8,
    scene: 'couple_portrait',
    budgetUsed: 0.1,
    borrowedFrom: [],
    userEdited: false,
    reviewed: false,
    modelVer: 100,
    analysisVer: 1,
    matrixVer: 1,
    ...overrides,
  };
}

function status(overrides: Partial<MicroStatusDto> = {}): MicroStatusDto {
  return {
    photos: 100,
    planned: 100,
    coverage: 1,
    actedOn: 40,
    regionCovered: 90,
    opCounts: [10, 5, 5, 8, 2],
    operators: OPERATORS,
    borrows: 0,
    withdrawnCounts: [0, 0, 0],
    families: FAMILIES,
    resolved: 0,
    meanCatchlightRatio: 0.99,
    meanHairEnergyRatio: 0.96,
    needsReview: 0,
    userEdited: 0,
    unlistedScenes: [],
    modelVer: 100,
    analysisVer: 1,
    matrixVer: 1,
    ...overrides,
  };
}

function matrix(overrides: Partial<MicroMatrixDto> = {}): MicroMatrixDto {
  return {
    allowed: [true, true, true, true, true],
    operators: OPERATORS,
    clothing: [true, true, true, false, false],
    clothingKinds: CLOTHING,
    clothingOptIn: [false, false, false, true, true],
    borrowing: true,
    ...overrides,
  };
}

function noop() {
  /* the handlers this test does not exercise */
}

function renderPanel(props: Partial<Parameters<typeof MicroRetouchPanel>[0]> = {}) {
  return render(
    <MicroRetouchPanel
      status={status()}
      plan={plan()}
      matrix={matrix()}
      onToggleOperation={noop}
      onToggleClothing={noop}
      onToggleBorrowing={noop}
      onAccept={noop}
      onCompare={noop}
      comparing={false}
      {...props}
    />,
  );
}

afterEach(cleanup);

describe('MicroRetouchPanel', () => {
  it('says what was done and what was held back', () => {
    renderPanel();
    expect(screen.getByTestId('micro-ops').textContent).toContain('stray hair');
    expect(screen.getByTestId('micro-withdrawals').textContent).toContain('too busy');
  });

  it('a gallery with no composites says so rather than staying silent', () => {
    renderPanel({ status: status({ borrows: 0 }) });
    expect(screen.getByTestId('micro-borrow-total').textContent).toContain(
      'No photograph in this gallery uses pixels from another one',
    );
  });

  it('a gallery with composites says how many, on the project header', () => {
    renderPanel({ status: status({ borrows: 3 }) });
    const total = screen.getByTestId('micro-borrow-total');
    expect(total.textContent).toContain('3 photographs');
    expect(total.className).toContain('is-composite');
  });

  it('a borrowed region is disclosed with its source and never as an ordinary edit', () => {
    renderPanel({
      plan: plan({
        ops: [
          op({
            kind: 'glare',
            method: 'borrow',
            borrowedFrom: 'pht_sibling',
            alignment: 0.93,
            region: { x: 0.37, y: 0.36, w: 0.04, h: 0.03 },
          }),
        ],
        borrowedFrom: ['pht_sibling'],
      }),
    });
    const disclosure = screen.getByTestId('micro-borrowed');
    expect(disclosure.textContent).toContain('pht_sibling');
    expect(disclosure.textContent).toContain('93%');
    // And the operation itself carries a badge, so a reader scanning the list cannot mistake it
    // for an ordinary fix.
    expect(screen.getByTestId('micro-op-badge-0').textContent).toContain('another frame');
  });

  it('marks the two opt-in clothing issues as opt-in', () => {
    renderPanel();
    expect(screen.getByTestId('micro-opt-in-strap')).toBeTruthy();
    expect(screen.getByTestId('micro-opt-in-crease')).toBeTruthy();
    expect(screen.queryByTestId('micro-opt-in-lint')).toBeNull();
  });

  it('switching an operation off reports the operator and the new state', () => {
    const onToggleOperation = vi.fn();
    renderPanel({ onToggleOperation });
    fireEvent.click(screen.getByTestId('micro-op-teeth'));
    expect(onToggleOperation).toHaveBeenCalledWith('teeth', false);
  });

  it('borrowing has its own switch, separate from glare', () => {
    const onToggleBorrowing = vi.fn();
    renderPanel({ onToggleBorrowing });
    fireEvent.click(screen.getByTestId('micro-borrowing'));
    expect(onToggleBorrowing).toHaveBeenCalledWith(false);
  });

  it('a withdrawn family is explained in the photographer own words', () => {
    renderPanel({
      plan: plan({
        naturalness: {
          catchlightRatio: 0.9,
          hairEnergyRatio: 0.99,
          teethExcursion: 0,
          measuredOn: 4096,
          resolves: 3,
          withdrawn: [false, false, true],
          families: FAMILIES,
        },
      }),
    });
    expect(screen.getByTestId('micro-caveats').textContent).toContain('the eye work');
  });

  it('says when the measurement is too small to mean much', () => {
    renderPanel({
      plan: plan({
        naturalness: {
          catchlightRatio: 1,
          hairEnergyRatio: 1,
          teethExcursion: 0,
          measuredOn: 11,
          resolves: 0,
          withdrawn: [false, false, false],
          families: FAMILIES,
        },
      }),
    });
    expect(screen.getByTestId('micro-caveats').textContent).toContain('rough');
  });

  it('says plainly when no regions arrived at all', () => {
    renderPanel({
      status: status({ regionCovered: 0, actedOn: 0 }),
      plan: plan({
        ops: [],
        reasons: [
          {
            code: 'micro_region_unavailable',
            text: 'AURA could not locate the regions it needs',
            weight: -0.25,
            doubt: true,
          },
        ],
      }),
    });
    expect(screen.getByTestId('micro-coverage').textContent).toContain(
      'None of them had the regions AURA needs',
    );
    expect(screen.getByTestId('micro-none')).toBeTruthy();
  });

  it('renders nothing about a photograph that has not been planned', () => {
    renderPanel({ plan: null });
    expect(screen.getByTestId('micro-empty')).toBeTruthy();
  });

  it('offers no control that could set a strength or raise a ceiling', () => {
    const { container } = renderPanel();
    // The structural half of `docs/retouch-ethics.md` section 4, asserted rather than reviewed:
    // every input on this surface is a switch.
    const inputs = Array.from(container.querySelectorAll('input'));
    expect(inputs.length).toBeGreaterThan(0);
    for (const input of inputs) {
      expect(input.getAttribute('type')).toBe('checkbox');
    }
  });
});
