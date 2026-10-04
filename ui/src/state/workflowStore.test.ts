import { describe, expect, it } from 'vitest';

import { useWorkflow } from './workflowStore';

describe('the workflow store', () => {
  it('starts every step as never-fetched, which is not the same as waiting', () => {
    const { evidence } = useWorkflow.getState();
    for (const step of Object.keys(evidence)) {
      expect(evidence[step as keyof typeof evidence].fetched).toBe(false);
    }
  });

  it('writes one step without disturbing the others', () => {
    useWorkflow.getState().setEvidence('cull', {
      status: 'done',
      hint: '412 selected.',
      fetched: true,
    });
    const { evidence } = useWorkflow.getState();
    expect(evidence.cull.status).toBe('done');
    expect(evidence.analyze.fetched).toBe(false);
  });

  it('forgets everything when a different wedding opens', () => {
    useWorkflow.getState().resetEvidence();
    const { evidence } = useWorkflow.getState();
    expect(evidence.cull.fetched).toBe(false);
    expect(evidence.cull.status).toBe('todo');
  });
});
