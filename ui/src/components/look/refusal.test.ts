import { expect, it } from 'vitest';
import { Refusal, reasonOf } from './refusal';

it('shows a refusal as written and a backend error by its own message', () => {
  expect(reasonOf(new Refusal('Another edit is running. Match this photo again when it finishes.')))
    .toBe('Another edit is running. Match this photo again when it finishes.');
  expect(reasonOf({ code: 'AURA-ML-5070', message: 'No safe colour fit.', runbookUrl: 'https://aura.app/e/AURA-ML-5070', retryable: false }))
    .toBe('No safe colour fit.');
});

it('never shows a raw exception to the photographer', () => {
  expect(reasonOf(new TypeError("Cannot read properties of undefined (reading 'id')")))
    .toBe('AURA hit an unexpected problem and stopped safely.');
});
