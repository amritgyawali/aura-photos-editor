import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { WelcomeScreen } from './WelcomeScreen';

describe('WelcomeScreen', () => {
  it('explains all five steps before there is a wedding', () => {
    render(
      <WelcomeScreen
        aiAnswered
        onCreateProject={() => undefined}
        onFirstRun={() => undefined}
      />,
    );
    for (const title of ['Import', 'Analyze', 'Cull', 'Edit', 'Export']) {
      expect(screen.getByText(title)).toBeDefined();
    }
  });

  it('starts a wedding under the name the product uses', () => {
    const onCreateProject = vi.fn();
    render(
      <WelcomeScreen
        aiAnswered
        onCreateProject={onCreateProject}
        onFirstRun={() => undefined}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Create a wedding' }));
    expect(onCreateProject).toHaveBeenCalledWith('Untitled wedding');
  });

  it('offers the tour and explains that local editing works before provider setup', () => {
    const onFirstRun = vi.fn();
    const view = render(
      <WelcomeScreen aiAnswered={false} onCreateProject={() => undefined} onFirstRun={onFirstRun} />,
    );
    expect(screen.getByText(/Local automatic editing is ready without setup/)).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'Show me how it works' }));
    expect(onFirstRun).toHaveBeenCalled();
    view.unmount();

    render(
      <WelcomeScreen aiAnswered onCreateProject={() => undefined} onFirstRun={() => undefined} />,
    );
    expect(screen.queryByText(/Local automatic editing is ready without setup/)).toBeNull();
  });
});
