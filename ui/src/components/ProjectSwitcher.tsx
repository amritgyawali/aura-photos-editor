import { useState } from 'react';

import type { ProjectSummary } from '../ipc/types';

export type ProjectSwitcherProps = {
  projects: ProjectSummary[];
  activeProjectId: string | null;
  onSelect: (projectId: string) => void;
  onCreate: (name: string) => void;
};

export function ProjectSwitcher({
  projects,
  activeProjectId,
  onSelect,
  onCreate,
}: ProjectSwitcherProps): JSX.Element {
  const [name, setName] = useState('');

  return (
    <section className="panel" aria-label="Weddings">
      <h2>Weddings</h2>

      <ul className="project-list">
        {projects.map((project) => (
          <li key={project.id}>
            <button
              type="button"
              className={project.id === activeProjectId ? 'project active' : 'project'}
              onClick={() => onSelect(project.id)}
            >
              <span className="project-name">{project.name}</span>
              <span className="project-count">{project.photoCount}</span>
            </button>
          </li>
        ))}
        {projects.length === 0 && <li className="empty">No weddings yet.</li>}
      </ul>

      <div className="row">
        <label htmlFor="project-name">New wedding</label>
        <input
          id="project-name"
          value={name}
          placeholder="Sarah and Tom"
          onChange={(event) => setName(event.target.value)}
        />
        <button
          type="button"
          disabled={name.trim().length === 0}
          onClick={() => {
            onCreate(name.trim());
            setName('');
          }}
        >
          Create
        </button>
      </div>
    </section>
  );
}
