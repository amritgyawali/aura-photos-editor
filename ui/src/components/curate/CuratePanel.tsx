import { useCallback, useEffect, useState } from 'react';

import { asIpcError, curate } from '../../ipc/client';
import type {
  CurateAlbumDto,
  CurateBwDto,
  CurateHeroDto,
  CuratePickDto,
  CurateSocialDto,
  CurateSpreadDto,
  CurateStatusDto,
} from '../../ipc/types';
import { AlbumBuilder } from './AlbumBuilder';
import { BwPicks } from './BwPicks';
import { HeroGrid } from './HeroGrid';
import { SocialSets } from './SocialSets';
import { SpreadView } from './SpreadView';

/**
 * PHASE-29. The container that wires the five curation views to the eleven curation commands.
 *
 * The five views are pure - rows and callbacks in, nothing fetched - which is what makes them
 * testable without a Tauri window. This is the one piece that talks to the shell. Phase 25's
 * `GalleryPanel` established the split; phases 26, 27 and 28 followed it, and so does this.
 *
 * ## One spread is fetched at a time
 *
 * `curateSpread` rather than reading the album's own copy, because the spread view is the screen a
 * photographer spends the most time on and the album is fetched to draw a list of numbers.
 *
 * ## Everything reloads after a write
 *
 * A reorder re-composes the album, which changes which two images share a spread and therefore every
 * pairing measurement after the moved frame. A panel that patched its own state would show a
 * photographer a pairing score AURA had not computed.
 *
 * ## The header says what could not be measured
 *
 * `rhythmMeasurable` and `headsTrained` are both on the status shape and both rendered, because on
 * this build the first is near zero and the second is false - and a panel that hid either would be
 * presenting a deterministic solver's answer over eight per cent of an album as a result.
 */
export type CuratePanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

/** Which view is showing. */
type Tab = 'album' | 'heroes' | 'bw' | 'social' | 'teaser';

/** The app banner's shape, from whatever the wire raised. */
function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-ML-5144',
    message: ipc?.message ?? 'The curation could not be read.',
  };
}

export function CuratePanel({ projectId, onError }: CuratePanelProps) {
  const [status, setStatus] = useState<CurateStatusDto | null>(null);
  const [album, setAlbum] = useState<CurateAlbumDto | null>(null);
  const [heroes, setHeroes] = useState<CurateHeroDto[]>([]);
  const [bw, setBw] = useState<CurateBwDto[]>([]);
  const [social, setSocial] = useState<CurateSocialDto | null>(null);
  const [teaser, setTeaser] = useState<CuratePickDto[]>([]);
  const [spread, setSpread] = useState<CurateSpreadDto | null>(null);
  const [tab, setTab] = useState<Tab>('album');
  const [running, setRunning] = useState(false);

  const reload = useCallback(async () => {
    if (!projectId) {
      setStatus(null);
      setAlbum(null);
      setHeroes([]);
      setBw([]);
      setSocial(null);
      setTeaser([]);
      setSpread(null);
      return;
    }
    try {
      const [nextStatus, nextAlbum, nextHeroes, nextBw, nextSocial, nextTeaser] = await Promise.all(
        [
          curate.curateStatus(projectId),
          curate.curateAlbum(projectId),
          curate.curateHeroes(projectId),
          curate.curateBw(projectId),
          curate.curateSocial(projectId),
          curate.curateTeaser(projectId),
        ],
      );
      setStatus(nextStatus);
      setAlbum(nextAlbum);
      setHeroes(nextHeroes);
      setBw(nextBw);
      setSocial(nextSocial);
      setTeaser(nextTeaser);
      onError(null);
    } catch (error) {
      onError(toBanner(error));
    }
  }, [projectId, onError]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const run = useCallback(async () => {
    if (!projectId) {
      return;
    }
    setRunning(true);
    try {
      await curate.curateProject({ projectId, albumSize: null });
      await reload();
    } catch (error) {
      onError(toBanner(error));
    } finally {
      setRunning(false);
    }
  }, [projectId, reload, onError]);

  const openSpread = useCallback(
    async (spreadId: string) => {
      try {
        setSpread(await curate.curateSpread(spreadId));
        onError(null);
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [onError],
  );

  const reorder = useCallback(
    async (order: string[]) => {
      if (!projectId) {
        return;
      }
      try {
        setAlbum(await curate.curateSetOrder({ projectId, order }));
        await reload();
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [projectId, reload, onError],
  );

  const decide = useCallback(
    async (imageId: string, kind: string, accepted: boolean) => {
      if (!projectId) {
        return;
      }
      try {
        await curate.curateDecide({ projectId, imageId, kind, accepted, note: null });
        await reload();
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [projectId, reload, onError],
  );

  if (!projectId) {
    return null;
  }

  return (
    <section className="curate-panel" aria-label="Curate">
      <header className="curate-header">
        <h3>Curate</h3>
        {status ? (
          <p className="curate-summary">
            {status.heroes} portfolio picks and a {status.albumSize}-image album, from{' '}
            {status.selected} delivered photographs.
            {status.selected > 0
              ? ` AURA could read ${(status.coverage * 100).toFixed(0)}% of them.`
              : ''}
          </p>
        ) : null}
        {status && status.rhythmMeasurable < 0.33 ? (
          <p className="curate-caveat">
            AURA could only tell how close the photographer was on{' '}
            {(status.rhythmMeasurable * 100).toFixed(0)}% of the album, so the rhythm score is not
            worth reading yet.
          </p>
        ) : null}
        {status && !status.headsTrained ? (
          <p className="curate-caveat">
            These suggestions come from measurements rather than from a trained model. AURA has not
            been shown enough real albums to learn a photographer&rsquo;s taste.
          </p>
        ) : null}
        <button type="button" onClick={() => void run()} disabled={running}>
          {running ? 'Curating…' : 'Curate this wedding'}
        </button>
      </header>

      <nav className="curate-tabs">
        {(['album', 'heroes', 'bw', 'social', 'teaser'] as Tab[]).map((name) => (
          <button
            key={name}
            type="button"
            aria-pressed={tab === name}
            onClick={() => setTab(name)}
          >
            {name === 'bw' ? 'black and white' : name}
          </button>
        ))}
      </nav>

      {tab === 'album' ? (
        <>
          <AlbumBuilder
            album={album}
            onSelectSpread={(id) => void openSpread(id)}
            onReorder={(order) => void reorder(order)}
          />
          <SpreadView spread={spread} />
        </>
      ) : null}

      {tab === 'heroes' ? (
        <HeroGrid heroes={heroes} onDecide={(id, ok) => void decide(id, 'hero', ok)} />
      ) : null}

      {tab === 'bw' ? (
        <BwPicks picks={bw} onDecide={(id, ok) => void decide(id, 'bw', ok)} />
      ) : null}

      {tab === 'social' ? (
        <SocialSets sets={social} onDecide={(id, kind, ok) => void decide(id, kind, ok)} />
      ) : null}

      {tab === 'teaser' ? (
        <section className="curate-teaser" aria-label="Teaser">
          {teaser.length === 0 ? (
            <p className="empty">No teaser yet.</p>
          ) : (
            <ol>
              {teaser.map((pick) => (
                <li key={pick.imageId} data-accepted={pick.accepted ?? 'undecided'}>
                  <span className="teaser-slot">{pick.slot}</span>
                  <span className="teaser-image">{pick.imageId}</span>
                  <ul className="teaser-reasons">
                    {pick.reasons.map((reason) => (
                      <li key={reason.code} className={reason.caveat ? 'caveat' : 'argument'}>
                        {reason.text}
                      </li>
                    ))}
                  </ul>
                  <span className="teaser-actions">
                    <button type="button" onClick={() => void decide(pick.imageId, 'teaser', true)}>
                      Send
                    </button>
                    <button
                      type="button"
                      onClick={() => void decide(pick.imageId, 'teaser', false)}
                    >
                      Skip
                    </button>
                  </span>
                </li>
              ))}
            </ol>
          )}
        </section>
      ) : null}
    </section>
  );
}
