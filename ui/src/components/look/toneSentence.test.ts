import { expect, it } from 'vitest';
import { toneSentence } from './toneSentence';

it('names a matte lift and a flat photo, and says nothing invented for an ordinary range', () => {
  expect(toneSentence({ lookBlack: 0.12, lookWhite: 0.93, ownBlack: 0, ownWhite: 1 }))
    .toBe("The reference's look lifts blacks by about 12% and softens whites to about 93%.");
  expect(toneSentence({ lookBlack: 0, lookWhite: 1, ownBlack: 0.15, ownWhite: 0.9 }))
    .toContain('flatter than a finished photo');
  expect(toneSentence({ lookBlack: 0.01, lookWhite: 0.99, ownBlack: 0, ownWhite: 1 }))
    .toBe('The reference has an ordinary tone range, so this photo keeps its own.');
});
