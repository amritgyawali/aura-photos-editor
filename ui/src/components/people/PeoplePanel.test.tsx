import { describe, expect, it } from 'vitest';

import type { GroupPeopleDto, IdentityCardDto, PeopleStatusDto, ScanFacesDto } from '../../ipc/types';
import {
  chooseSurvivor,
  mergeSummary,
  suspiciousMerge,
} from './MergeDialog';
import {
  faceCountLabel,
  importanceLabel,
  roleLabel,
  roleProvenance,
  varianceNote,
} from './IdentityCard';
import {
  couplePrompt,
  coverageSentence,
  gatedSentence,
  groupSentence,
  scanSentence,
  tileSentence,
} from './PeoplePanel';

const status = (over: Partial<PeopleStatusDto> = {}): PeopleStatusDto => ({
  photos: 400,
  scanned: 400,
  coverage: 1,
  faces: 960,
  votingFaces: 700,
  identities: 24,
  tiledFrames: 40,
  coupleUnconfirmed: false,
  staleVersions: [],
  erased: false,
  keyStore: 'windows-dpapi',
  weightsVer: 1,
  ...over,
});

const identity = (over: Partial<IdentityCardDto> = {}): IdentityCardDto => ({
  id: 'idt_11111111-1111-1111-1111-111111111111',
  label: null,
  role: 'guest',
  roleConfidence: 0.45,
  roleReasons: ['appears in 6 photographs with little couple co-occurrence'],
  userLocked: false,
  importance: 0.5,
  faces: 6,
  votingFaces: 6,
  frames: 6,
  firstSeen: null,
  lastSeen: null,
  meanQuality: 0.7,
  coverFaceId: null,
  variance: 0.1,
  subCount: 0,
  companions: [],
  ...over,
});

describe('coverageSentence', () => {
  it('says an unscanned project is unscanned rather than empty', () => {
    const line = coverageSentence(status({ scanned: 0, faces: 0, identities: 0 }));
    expect(line).toContain('have been read for faces yet');
  });

  it('names erasure rather than reporting no faces', () => {
    // "There are no faces" and "the faces were deliberately destroyed" are different
    // answers to a client who asks, and the panel must not confuse them.
    const line = coverageSentence(status({ erased: true, faces: 0, identities: 0 }));
    expect(line).toContain('erased');
    expect(line).toContain('kept');
  });

  it('reports the stale-version re-read rather than hiding it', () => {
    const line = coverageSentence(status({ staleVersions: [100100] }));
    expect(line).toContain('older model');
  });

  it('handles a project with no photographs', () => {
    expect(coverageSentence(status({ photos: 0, scanned: 0 }))).toContain('no photographs');
  });
});

describe('gatedSentence', () => {
  it('explains gated faces so the count difference is not read as a bug', () => {
    const line = gatedSentence(status());
    expect(line).not.toBeNull();
    expect(line).toContain('260 of 960');
    expect(line).toContain('still count');
  });

  it('says nothing when every face votes', () => {
    expect(gatedSentence(status({ votingFaces: 960 }))).toBeNull();
  });
});

describe('tileSentence', () => {
  it('reports the tiled-pass cost as a percentage', () => {
    expect(tileSentence(status())).toContain('10%');
  });

  it('says nothing when no frame was tiled', () => {
    expect(tileSentence(status({ tiledFrames: 0 }))).toBeNull();
  });
});

describe('couplePrompt', () => {
  const grouping = (over: Partial<GroupPeopleDto> = {}): GroupPeopleDto => ({
    identities: 24,
    assigned: 700,
    unassigned: 12,
    threshold: 0.55,
    refusedMerges: 2,
    subClustered: 1,
    couple: [],
    coupleConfidence: 0.6,
    coupleAmbiguous: false,
    sceneStarved: false,
    decisionsReplayed: 0,
    decisionsOrphaned: 0,
    coverage: 1,
    elapsedMs: 900,
    ...over,
  });

  it('asks nothing once the couple is confirmed', () => {
    expect(couplePrompt(status({ coupleUnconfirmed: false }), grouping())).toBeNull();
  });

  it('asks nothing when there is nobody to ask about', () => {
    expect(
      couplePrompt(status({ coupleUnconfirmed: true, identities: 0 }), grouping()),
    ).toBeNull();
  });

  it('says the two pairs were close when they were', () => {
    const line = couplePrompt(
      status({ coupleUnconfirmed: true }),
      grouping({ coupleAmbiguous: true }),
    );
    expect(line).toContain('almost the same');
  });

  it('says scenes are missing when they are, rather than sounding confident', () => {
    const line = couplePrompt(
      status({ coupleUnconfirmed: true }),
      grouping({ sceneStarved: true }),
    );
    expect(line).toContain('cannot yet tell');
  });

  it('still asks when nothing has been grouped yet in this session', () => {
    expect(couplePrompt(status({ coupleUnconfirmed: true }), null)).toContain('Confirm');
  });
});

describe('roleLabel and roleProvenance', () => {
  it('never states which half of the couple somebody is', () => {
    // Section 6.3: the evidence identifies a pair, not a bride. The card must invite a
    // confirmation rather than assert one.
    expect(roleLabel(identity({ role: 'couple' }))).toContain('confirm which');
  });

  it('says who decided when the photographer decided', () => {
    const line = roleProvenance(identity({ userLocked: true, role: 'bride' }));
    expect(line).toContain('set by you');
    expect(line).not.toContain('%');
  });

  it('gives the confidence when AURA decided', () => {
    expect(roleProvenance(identity({ roleConfidence: 0.62 }))).toContain('62%');
  });
});

describe('faceCountLabel', () => {
  it('accounts for gated faces rather than hiding them', () => {
    const line = faceCountLabel(identity({ faces: 9, votingFaces: 6, frames: 7 }));
    expect(line).toContain('7 photographs');
    expect(line).toContain('3 faces too small');
  });

  it('says only the frame count when nothing was gated', () => {
    expect(faceCountLabel(identity({ frames: 1, faces: 1, votingFaces: 1 }))).toBe(
      '1 photograph',
    );
  });
});

describe('varianceNote', () => {
  it('warns when two looks were grouped, because that is also what a bad merge looks like', () => {
    const note = varianceNote(identity({ subCount: 2, variance: 0.41 }));
    expect(note).toContain('split it');
  });

  it('says nothing for a single-look identity', () => {
    expect(varianceNote(identity())).toBeNull();
  });
});

describe('importanceLabel', () => {
  it('calls an untouched slider normal', () => {
    expect(importanceLabel(0.5)).toBe('Normal');
  });

  it('names the direction either way', () => {
    expect(importanceLabel(0.9)).toContain('More important');
    expect(importanceLabel(0.2)).toContain('Less important');
  });
});

describe('chooseSurvivor', () => {
  it('keeps the named identity even when it has fewer faces', () => {
    // Somebody typed that name. Losing it to a face count would discard the
    // photographer's work silently, which is the whole point of this function.
    const named = identity({ id: 'idt_a', label: 'Priya', faces: 2 });
    const unnamed = identity({ id: 'idt_b', faces: 40 });
    expect(chooseSurvivor(unnamed, named).survivor.id).toBe('idt_a');
  });

  it('keeps a locked identity over an unlocked one', () => {
    const locked = identity({ id: 'idt_a', userLocked: true, faces: 3 });
    const loose = identity({ id: 'idt_b', faces: 30 });
    expect(chooseSurvivor(loose, locked).survivor.id).toBe('idt_a');
  });

  it('falls back to the larger identity, then to the id', () => {
    const big = identity({ id: 'idt_z', faces: 30 });
    const small = identity({ id: 'idt_a', faces: 3 });
    expect(chooseSurvivor(small, big).survivor.id).toBe('idt_z');

    const left = identity({ id: 'idt_a', faces: 5 });
    const right = identity({ id: 'idt_b', faces: 5 });
    expect(chooseSurvivor(right, left).survivor.id).toBe('idt_a');
  });
});

describe('mergeSummary', () => {
  it('names what is kept and what goes, before the photographer commits', () => {
    const named = identity({ id: 'idt_a', label: 'Priya', faces: 4 });
    const unnamed = identity({ id: 'idt_b', faces: 6 });
    const line = mergeSummary(named, unnamed);
    expect(line).toContain('6 face(s) will move');
    expect(line).toContain('Priya keeps its name');
    expect(line).toContain('undo');
  });
});

describe('suspiciousMerge', () => {
  it('warns when two confident identities were never photographed together', () => {
    const left = identity({ id: 'idt_a', roleConfidence: 0.8 });
    const right = identity({ id: 'idt_b', roleConfidence: 0.7 });
    expect(suspiciousMerge(left, right)).toContain('two different people');
  });

  it('does not warn when they were photographed together', () => {
    const right = identity({ id: 'idt_b', roleConfidence: 0.7 });
    const left = identity({
      id: 'idt_a',
      roleConfidence: 0.8,
      companions: [{ id: 'idt_b', label: null, frames: 12 }],
    });
    expect(suspiciousMerge(left, right)).toBeNull();
  });

  it('does not warn when neither role is confident', () => {
    const left = identity({ id: 'idt_a', roleConfidence: 0.2 });
    const right = identity({ id: 'idt_b', roleConfidence: 0.3 });
    expect(suspiciousMerge(left, right)).toBeNull();
  });
});

describe('scanSentence and groupSentence', () => {
  const scan: ScanFacesDto = {
    scanned: 400,
    faces: 960,
    voting: 700,
    gated: 260,
    personBoxes: 1100,
    headless: 140,
    failed: 2,
    remaining: 0,
    tiledFrames: 40,
    tileRatio: 0.1,
    elapsedMs: 74_000,
    cancelled: false,
  };

  it('reports headless people, because that is what body detection is for', () => {
    expect(scanSentence(scan)).toContain('140 people with no visible face');
  });

  it('reports failures rather than swallowing them', () => {
    expect(scanSentence(scan)).toContain('2 could not be read');
  });

  it('reports refused merges and kept decisions', () => {
    const line = groupSentence({
      identities: 24,
      assigned: 700,
      unassigned: 12,
      threshold: 0.55,
      refusedMerges: 3,
      subClustered: 1,
      couple: [],
      coupleConfidence: 0.6,
      coupleAmbiguous: false,
      sceneStarved: false,
      decisionsReplayed: 4,
      decisionsOrphaned: 1,
      coverage: 1,
      elapsedMs: 1_200,
    });
    expect(line).toContain('12 left unassigned rather than guessed at');
    expect(line).toContain('3 risky merge(s) refused');
    expect(line).toContain('4 of your decisions kept');
    expect(line).toContain('1 of your decisions could not be reapplied');
  });
});
