import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../../ipc/client';
import type { ChapterDto, StoryOutlineDto } from '../../ipc/types';
import { boundaryRange, boundarySummary, canMerge, clampBoundary, mergeSummary } from './BoundaryEditor';
import { Timeline } from './Timeline';

/**
 * PHASE-07. The container that puts the chapter editor beside the timeline.
 *
 * `Timeline` reads the story and draws it; the four editing commands - rename, move a boundary,
 * split and merge - had no view at all until this file, and `BoundaryEditor`'s helpers, which
 * have their own tests, were imported by nothing (`PHASE-01-30-REVIEW.md` section 6.4).
 *
 * **The editor makes the backend's refusals unreachable rather than discovering them.** A
 * boundary drag is clamped into the legal range before it is sent, a non-adjacent merge is
 * disabled rather than refused, and every control says what it will do before it does it. The
 * backend still refuses - `AURA-ML-5025` is what makes the guarantee real - but a photographer
 * should never be the one to find that out.
 *
 * **Moving a boundary locks both sides.** ADR-0016 decision 2, and `boundarySummary` says so in
 * the sentence, because a photographer who thinks they are editing one chapter will be surprised
 * when the next re-analysis leaves the other one alone too.
 */
export type StoryPanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-ML-5020',
    message: ipc?.message ?? 'The story could not be read.',
  };
}

export function StoryPanel({ projectId, onError }: StoryPanelProps): JSX.Element {
  const [outline, setOutline] = useState<StoryOutlineDto | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [proposed, setProposed] = useState<number | null>(null);
  const [label, setLabel] = useState('');
  const [busy, setBusy] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  const fail = useCallback(
    (error: unknown) => {
      onError(toBanner(error));
    },
    [onError],
  );

  const reload = useCallback(async () => {
    if (!projectId || !inTauri()) {
      setOutline(null);
      return;
    }
    try {
      setOutline(await api.storyOutline(projectId));
    } catch (error) {
      fail(error);
    }
  }, [fail, projectId]);

  useEffect(() => {
    void reload();
  }, [reload, reloadKey]);

  const write = useCallback(
    async (action: () => Promise<unknown>) => {
      setBusy(true);
      try {
        await action();
        onError(null);
        setProposed(null);
        setReloadKey((key) => key + 1);
      } catch (error) {
        fail(error);
      } finally {
        setBusy(false);
      }
    },
    [fail, onError],
  );

  const chapters: ChapterDto[] = outline?.chapters ?? [];
  const index = chapters.findIndex((chapter) => chapter.segmentId === selected);
  const chapter = index >= 0 ? chapters[index] : undefined;
  const next = index >= 0 ? chapters[index + 1] : undefined;
  const range = chapter ? boundaryRange(chapter, next) : null;
  const summary =
    chapter && proposed !== null ? boundarySummary(chapter, next, proposed) : null;

  if (!projectId) {
    return (
      <section className="story-panel">
        <p className="empty">Open a wedding to see the day as chapters.</p>
      </section>
    );
  }

  return (
    <section className="story-panel" aria-label="Story">
      <Timeline projectId={projectId} />

      <section className="story-panel__editor" aria-label="Chapter editor">
        <h3>Edit a chapter</h3>
        {busy ? <p role="status">Saving…</p> : null}

        <label>
          Chapter
          <select
            value={selected ?? ''}
            onChange={(event) => {
              const id = event.target.value || null;
              setSelected(id);
              setProposed(null);
              setLabel(chapters.find((row) => row.segmentId === id)?.label ?? '');
            }}
          >
            <option value="">Choose a chapter…</option>
            {chapters.map((row) => (
              <option key={row.segmentId} value={row.segmentId}>
                {row.label ?? row.title} · {row.imageCount} photographs
              </option>
            ))}
          </select>
        </label>

        {chapter ? (
          <>
            <label>
              Your name for it
              <input
                type="text"
                value={label}
                placeholder={chapter.title}
                onChange={(event) => setLabel(event.target.value)}
              />
            </label>
            <button
              type="button"
              disabled={busy}
              onClick={() =>
                void write(() =>
                  api.setChapter({
                    segmentId: chapter.segmentId,
                    chapter: chapter.chapter,
                    label: label.trim() === '' ? null : label.trim(),
                  }),
                )
              }
            >
              Rename
            </button>

            {range ? (
              <>
                <label>
                  Where this chapter ends
                  <input
                    type="range"
                    min={range.min}
                    max={range.max}
                    step={1000}
                    value={proposed ?? chapter.endMs}
                    onChange={(event) =>
                      setProposed(
                        clampBoundary(Number(event.target.value), chapter, next) ?? chapter.endMs,
                      )
                    }
                  />
                </label>
                {summary ? <p className="story-panel__summary">{summary}</p> : null}
                <button
                  type="button"
                  disabled={busy || proposed === null}
                  onClick={() => {
                    if (proposed === null) {
                      return;
                    }
                    void write(() =>
                      api.moveChapterBoundary({
                        segmentId: chapter.segmentId,
                        newEndMs: proposed,
                      }),
                    );
                  }}
                >
                  Move the boundary
                </button>
              </>
            ) : (
              <p className="story-panel__summary">
                This is the last chapter of the day, so there is no boundary after it to move.
              </p>
            )}

            {next ? (
              <>
                <p className="story-panel__summary">{mergeSummary(chapter, next)}</p>
                <button
                  type="button"
                  disabled={busy || !canMerge(chapter, next)}
                  onClick={() =>
                    void write(() =>
                      api.mergeChapters({
                        segmentIdA: chapter.segmentId,
                        segmentIdB: next.segmentId,
                      }),
                    )
                  }
                >
                  Merge with “{next.label ?? next.title}”
                </button>
              </>
            ) : null}
          </>
        ) : null}
      </section>
    </section>
  );
}
