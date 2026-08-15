import { useCallback, useEffect, useState } from 'react';

import { emotion as emotionApi, asIpcError } from '../../ipc/client';
import type {
  CropRectDto,
  EmotionDto,
  EmotionReasonDto,
  FaceExpressionDto,
  InteractionDto,
  ReactionLinkDto,
} from '../../ipc/types';

/**
 * The Emotion card: what one photograph is worth, and the faces that say so.
 *
 * PHASE-10 section 9's SFE task - "Emotion card with face crops, interaction chips, peak
 * indicator" - and section 13's fifth acceptance criterion: "the Emotion card explains
 * the score with crops and short editorial reasons."
 *
 * ## The four rules this card is built around
 *
 * **It describes photographs, never people.** Section 2.2 puts any claim about a
 * person's inner state permanently out of scope. Every label here is written about the
 * frame - "this face reads as crying", not "she was moved" - and the vocabulary is
 * closed: the sentences come from `EmotionCode::user_text` on the Rust side, and this
 * file writes none of its own about a subject.
 *
 * **A missing reading is not a flat reading.** `imageEmotion` returns `null` when nobody
 * has looked, and the card says so in those words. Drawing a zero score would tell a
 * photographer that AURA looked and found nothing happening - which is the confusion
 * migration 10 spends its sixth stated property preventing in the schema, carried out to
 * the interface. Phase 09's card makes the same distinction for the same reason.
 *
 * **Composure is shown as a positive.** The eight channel bars are drawn in the model's
 * own order with no re-ranking, and `composed` is not styled as an absence. A card that
 * greyed it out would undo in the interface what `emotion_weights.toml` spends two
 * hundred lines establishing.
 *
 * **Nothing here decides anything.** There is no keep button, no reject button and no
 * star. The two controls are "I would deliver this one" and "this frame is the one",
 * and neither changes what is delivered. Section 2.2 puts the choosing in phase 12, and
 * an ordering by emotion is the place it would be most tempting to start early.
 */

/** How an emotion score reads to a photographer. */
export function scoreLabel(score: number): string {
  if (score >= 0.75) {
    return 'a strong moment';
  }
  if (score >= 0.5) {
    return 'worth a look';
  }
  if (score >= 0.25) {
    return 'quiet';
  }
  return 'nothing in particular';
}

/**
 * The headline sentence, which says what was found before it says what it scored.
 *
 * A photographer reading "0.71" wants to know what that is 0.71 *of*. The strongest
 * credited reason is the answer, and the card's own ordering supplies it.
 */
export function headline(reading: EmotionDto): string {
  const best = reading.reasons.find((reason) => !reason.caveat && reason.weight > 0);
  const label = scoreLabel(reading.emotionScore);
  if (!best) {
    return `AURA reads this as ${label}.`;
  }
  return `AURA reads this as ${label}, mostly because ${best.text}.`;
}

/** The sentence shown above a photograph nobody has scored. */
export function notScoredSentence(): string {
  return 'AURA has not read this photograph yet, so it carries no emotion score. That is not the same as finding nothing happening in it.';
}

/**
 * What the confidence means, in words.
 *
 * The thresholds are the card's own and are only ever used to choose a sentence. Nothing
 * downstream reads them.
 */
export function confidenceSentence(reading: EmotionDto): string {
  if (reading.confidence >= 0.7) {
    return 'AURA is confident about this reading.';
  }
  if (reading.confidence >= 0.4) {
    return 'AURA is moderately sure about this reading.';
  }
  return 'AURA is not sure about this reading; the reasons below say what was missing.';
}

/**
 * The score as a percentage string.
 *
 * One decimal place would be false precision on a number that comes out of a logistic
 * over nine features, three of which are missing on most frames.
 */
export function percent(value: number): string {
  return `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;
}

/**
 * A crop as a CSS inset, in percentages.
 *
 * Percentages rather than pixels, for `IntegrityCard`'s reason: the same rectangle then
 * addresses a 240 px thumbnail, a 2048 px proxy and the original without the card
 * knowing which one is on screen.
 */
export function cropStyle(crop: CropRectDto): {
  left: string;
  top: string;
  width: string;
  height: string;
} {
  return {
    left: `${crop.x * 100}%`,
    top: `${crop.y * 100}%`,
    width: `${crop.w * 100}%`,
    height: `${crop.h * 100}%`,
  };
}

/**
 * How a gap in milliseconds reads.
 *
 * Signed, and the sign is kept: a reaction frame that is *earlier* than its action is a
 * real and common case across two cameras, and rounding it away would hide a clock-skew
 * problem the photographer might want to know about.
 */
export function gapSentence(gapMs: number): string {
  const seconds = Math.abs(gapMs) / 1000;
  const rounded = seconds < 1 ? seconds.toFixed(1) : Math.round(seconds).toString();
  return gapMs >= 0 ? `${rounded}s after` : `${rounded}s before`;
}

/** The eight channel bars, in the model's own order. */
export function ChannelBars({
  face,
  names,
}: {
  face: FaceExpressionDto;
  names: string[];
}): JSX.Element {
  return (
    <ul className="emotion-card__channels" aria-label="expression readings">
      {face.channels.map((value, index) => {
        const name = names[index] ?? `channel ${index}`;
        return (
          <li key={name} className="emotion-card__channel">
            <span className="emotion-card__channel-name">{name}</span>
            <span
              className="emotion-card__channel-bar"
              data-channel={name}
              style={{ width: percent(value) }}
              role="img"
              aria-label={`${name} ${percent(value)}`}
            />
            <span className="emotion-card__channel-value">{percent(value)}</span>
          </li>
        );
      })}
    </ul>
  );
}

/** One face, with its crop, its gaze and its eight readings. */
export function FaceRow({
  face,
  names,
}: {
  face: FaceExpressionDto;
  names: string[];
}): JSX.Element {
  return (
    <li className="emotion-card__face" data-face-id={face.faceId}>
      <span
        className="emotion-card__face-crop"
        style={cropStyle(face.crop)}
        data-testid="face-crop"
      />
      <div className="emotion-card__face-body">
        <p className="emotion-card__face-gaze">
          {face.gaze === 'unknown'
            ? 'AURA could not tell where this face is looking.'
            : `Looking at the ${face.gaze}.`}
        </p>
        {face.readsAsCrying ? (
          <p className="emotion-card__face-note" data-note="tears">
            This face reads as crying.
          </p>
        ) : null}
        {face.posedSmile ? (
          <p className="emotion-card__face-note" data-note="posed">
            The smile here reads as held for the camera.
          </p>
        ) : null}
        <ChannelBars face={face} names={names} />
      </div>
    </li>
  );
}

/** The interaction chips. */
export function InteractionChips({ items }: { items: InteractionDto[] }): JSX.Element | null {
  if (items.length === 0) {
    return null;
  }
  return (
    <ul className="emotion-card__interactions" aria-label="what is happening">
      {items.map((item) => (
        <li
          key={item.kind}
          className="emotion-card__chip"
          data-kind={item.kind}
          data-milestone={item.milestone ? 'yes' : 'no'}
        >
          {item.title} · {percent(item.strength)}
        </li>
      ))}
    </ul>
  );
}

/**
 * The peak indicator.
 *
 * Three states rather than two, because "this moment has no clear best frame" is a
 * common and correct answer that a two-state indicator would have to round into one of
 * the wrong ones.
 */
export function PeakIndicator({ proximity }: { proximity: number }): JSX.Element {
  const state = proximity >= 0.85 ? 'at-peak' : proximity <= 0.55 ? 'off-peak' : 'near-peak';
  const sentence =
    state === 'at-peak'
      ? 'This is where the action in its moment is strongest.'
      : state === 'off-peak'
        ? 'This sits either side of the strongest frame in its moment.'
        : 'This is close to the strongest frame in its moment.';
  return (
    <p className="emotion-card__peak" data-state={state}>
      {sentence}
    </p>
  );
}

/** One reason, with its crop when it has one. */
export function ReasonRow({ reason }: { reason: EmotionReasonDto }): JSX.Element {
  return (
    <li
      className="emotion-card__reason"
      data-code={reason.code}
      data-caveat={reason.caveat ? 'yes' : 'no'}
    >
      {reason.evidence ? (
        <span
          className="emotion-card__reason-crop"
          style={cropStyle(reason.evidence)}
          data-testid="reason-crop"
        />
      ) : null}
      <span className="emotion-card__reason-text">{reason.text}</span>
    </li>
  );
}

/** The reaction pair viewer: what reacted to this frame, or what this frame reacts to. */
export function ReactionPairs({
  links,
  reactionOf,
}: {
  links: ReactionLinkDto[];
  reactionOf?: string | null;
}): JSX.Element | null {
  if (links.length === 0 && !reactionOf) {
    return null;
  }
  return (
    <section className="emotion-card__reactions" aria-label="reactions">
      {reactionOf ? (
        <p className="emotion-card__reaction-of" data-action={reactionOf}>
          This frame is somebody reacting to another moment.
        </p>
      ) : null}
      <ul>
        {links.map((link) => (
          <li key={link.reaction} className="emotion-card__reaction" data-photo={link.reaction}>
            Somebody reacted to this, {gapSentence(link.gapMs)}.
          </li>
        ))}
      </ul>
    </section>
  );
}

/** What the card needs from its parent. */
export type EmotionCardProps = {
  photoId: string;
  /** Injected in tests. Live code lets the card fetch. */
  reading?: EmotionDto | null;
  /** Injected in tests. */
  links?: ReactionLinkDto[];
};

/** The card. */
export function EmotionCard({ photoId, reading, links }: EmotionCardProps): JSX.Element {
  const [loaded, setLoaded] = useState<EmotionDto | null | undefined>(reading);
  const [reactions, setReactions] = useState<ReactionLinkDto[]>(links ?? []);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [next, incoming] = await Promise.all([
        emotionApi.imageEmotion(photoId),
        emotionApi.reactionsOf(photoId),
      ]);
      setLoaded(next);
      setReactions(incoming);
      setError(null);
    } catch (thrown) {
      setError(asIpcError(thrown).message);
    }
  }, [photoId]);

  useEffect(() => {
    if (reading === undefined) {
      void load();
    }
  }, [load, reading]);

  if (error) {
    return (
      <section className="emotion-card emotion-card--error" aria-label="emotion">
        <p>{error}</p>
      </section>
    );
  }

  if (loaded === undefined) {
    return (
      <section className="emotion-card emotion-card--loading" aria-label="emotion">
        <p>Reading this photograph…</p>
      </section>
    );
  }

  // `null` is "nobody looked", which the card says in those words rather than drawing a
  // zero. See this file's header.
  if (loaded === null) {
    return (
      <section className="emotion-card emotion-card--unscored" aria-label="emotion">
        <p>{notScoredSentence()}</p>
      </section>
    );
  }

  return (
    <section className="emotion-card" aria-label="emotion" data-photo-id={loaded.photoId}>
      <header className="emotion-card__header">
        <p className="emotion-card__headline">{headline(loaded)}</p>
        <p className="emotion-card__score" data-score={loaded.emotionScore.toFixed(3)}>
          {percent(loaded.emotionScore)}
        </p>
        <p className="emotion-card__confidence">{confidenceSentence(loaded)}</p>
        {loaded.narrativeWeight > 0 ? (
          <p className="emotion-card__narrative" data-weight={loaded.narrativeWeight.toFixed(2)}>
            A cloud model rated this moment&rsquo;s place in the day at{' '}
            {percent(loaded.narrativeWeight)}.
          </p>
        ) : null}
      </header>

      <InteractionChips items={loaded.interactions} />
      {loaded.mutualGaze ? (
        <p className="emotion-card__mutual">Two people here are looking at each other.</p>
      ) : null}
      <PeakIndicator proximity={loaded.peakProximity} />

      <ul className="emotion-card__reasons" aria-label="why">
        {loaded.reasons.map((reason) => (
          <ReasonRow key={`${reason.code}-${reason.text}`} reason={reason} />
        ))}
      </ul>

      {loaded.faces.length > 0 ? (
        <ul className="emotion-card__faces" aria-label="faces">
          {loaded.faces.map((face) => (
            <FaceRow key={face.faceId} face={face} names={loaded.channelNames} />
          ))}
        </ul>
      ) : null}

      <ReactionPairs links={reactions} reactionOf={loaded.reactionOf} />

      <footer className="emotion-card__footer">
        <p className="emotion-card__scene">
          Weighted for <strong>{loaded.scene}</strong>.
        </p>
        <p className="emotion-card__versions">
          heads {loaded.modelVer} · analysis {loaded.analysisVer} · weights {loaded.weightsVer}
        </p>
      </footer>
    </section>
  );
}
