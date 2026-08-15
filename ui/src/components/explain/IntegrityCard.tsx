import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError } from '../../ipc/client';
import type { CropRectDto, EyeStateDto, IntegrityDto, IntegrityReasonDto } from '../../ipc/types';

/**
 * The Integrity card: what is technically wrong with one photograph, and the crop that
 * proves each claim.
 *
 * PHASE-09 section 9's SFE task, and section 13's last acceptance criterion: "the
 * Integrity card shows the exact crop that caused each penalty."
 *
 * ## The three rules this card is built around
 *
 * **A missing verdict is not a clean verdict.** `imageIntegrity` returns `null` when
 * nobody has looked at the photograph, and this card says so in those words. Drawing an
 * empty card would tell a photographer that AURA checked and found nothing - which is
 * the exact confusion migration 9 spends a paragraph preventing in the schema, carried
 * out to the interface.
 *
 * **The good news is shown as prominently as the bad.** Eight of the twenty-one reason
 * codes withdraw a claim rather than making one, and two of the fourteen flags describe
 * something *right* with a photograph. A card that listed only penalties would be a card
 * that never explains why a shallow-depth-of-field portrait was left alone - and section
 * 12's first failure mode is that false rejections destroy trust, which is a thing a
 * photographer has to be able to *see* not happening.
 *
 * **Nothing here decides anything.** There is no keep button, no reject button and no
 * ranking. The one control is "this mark is wrong", which clears a flag and changes
 * nothing about delivery. Section 2.2 puts culling in phase 12 and this card is the
 * place it would be most tempting to start early.
 */

/** How a technical score reads to a photographer. */
export function scoreLabel(score: number): string {
  if (score >= 0.85) {
    return 'technically clean';
  }
  if (score >= 0.65) {
    return 'usable';
  }
  if (score >= 0.4) {
    return 'has a problem';
  }
  return 'badly affected';
}

/**
 * The headline sentence, which says what was measured before it says what was found.
 *
 * A photographer reading "0.31" wants to know what that is 0.31 *of*. Naming the weakest
 * of the five sub-scores is the answer, and the card's own ordering of reasons supplies
 * it: the strongest penalty is first.
 */
export function headline(verdict: IntegrityDto): string {
  const worst = verdict.reasons.find((reason) => reason.weight < 0);
  const label = scoreLabel(verdict.technicalScore);
  if (!worst) {
    return `This frame is ${label}. Nothing counted against it.`;
  }
  return `This frame is ${label}, mostly because ${worst.text}.`;
}

/** The sentence shown above a photograph nobody has analysed. */
export function notCheckedSentence(): string {
  return 'AURA has not checked this photograph yet, so it carries no technical marks. That is not the same as finding nothing wrong.';
}

/**
 * What the confidence means, in words.
 *
 * The same three-band shape the duplicate panel uses, for the same reason: a number
 * between 0 and 1 does not tell a photographer whether to look.
 */
export function confidenceSentence(verdict: IntegrityDto): string {
  const reasons: string[] = [];
  if (verdict.flags.includes('no_subject_detected')) {
    reasons.push('no face or person was found to judge against');
  }
  if (verdict.reasons.some((reason) => reason.code === 'uncalibrated')) {
    reasons.push('this camera model has not been measured yet');
  }
  if (verdict.scene === 'unknown') {
    reasons.push('this photograph has no scene label');
  }
  if (reasons.length === 0) {
    return `Confidence ${verdict.confidence.toFixed(2)}.`;
  }
  return `Confidence ${verdict.confidence.toFixed(2)}, lowered because ${reasons.join(', and ')}.`;
}

/**
 * The exposure verdict as a photographer would say it.
 *
 * Four words rather than the enum's four slugs, because `marginal` is jargon and
 * `recoverable` reads as a promise the phase is careful not to make about a *specific*
 * edit.
 */
export function exposureSentence(verdict: IntegrityDto): string {
  const stops = verdict.evOffset;
  const direction = stops < 0 ? 'under' : 'over';
  const amount = Math.abs(stops) < 0.2 ? '' : ` (about ${Math.abs(stops).toFixed(1)} stops ${direction})`;
  switch (verdict.exposure) {
    case 'good':
      return `The exposure needs nothing${amount}.`;
    case 'recoverable':
      return `The exposure can be brought back on this camera${amount}.`;
    case 'marginal':
      return `The exposure can be corrected, and it will cost some quality${amount}.`;
    default:
      return `Some of the exposure is not in the file to recover${amount}.`;
  }
}

/**
 * The noise reading, expressed against the scene rather than absolutely.
 *
 * `noiseSigmaRel` is already scene-relative - 1.0 *is* the tolerance - so the sentence
 * says so rather than quoting a sigma nobody can interpret.
 */
export function noiseSentence(verdict: IntegrityDto): string {
  if (verdict.noiseSigmaRel <= 0) {
    return 'There was no flat area large enough to measure noise in.';
  }
  const percent = Math.round(verdict.noiseSigmaRel * 100);
  if (verdict.noiseSigmaRel <= 1) {
    return `Noise is at ${percent}% of what this kind of photograph carries well.`;
  }
  return `Noise is at ${percent}% of what this kind of photograph carries well - more than usual for this scene.`;
}

/**
 * The eye summary, which is the sentence photographers read first.
 *
 * The denominator is always stated. "Nobody is blinking" and "nobody's eyes were judged"
 * are different facts and the card must not blur them.
 */
export function eyeSentence(verdict: IntegrityDto): string {
  if (verdict.gatingFaces === 0) {
    return 'Nobody in this frame is prominent enough for their eyes to count against it.';
  }
  const closed = verdict.eyes.filter((eye) => eye.gates && eye.state === 'closed');
  if (closed.length === 0) {
    return `Eyes open on all ${verdict.gatingFaces} of the people this frame is about.`;
  }
  const justified = closed.filter((eye) => eye.intentional).length;
  if (justified === closed.length) {
    return `${closed.length} of ${verdict.gatingFaces} have their eyes closed, and this moment explains it.`;
  }
  return `${closed.length - justified} of ${verdict.gatingFaces} have their eyes closed with nothing to explain it.`;
}

/**
 * The reasons, penalties first and exonerations after.
 *
 * The backend already sorts by weight, so a penalty of -0.4 leads. This regroups so the
 * good news is not interleaved with the bad - a card that alternated "her eyes are
 * closed" and "the background is soft on purpose" reads as indecision.
 */
export function orderedReasons(verdict: IntegrityDto): IntegrityReasonDto[] {
  const penalties = verdict.reasons.filter((reason) => reason.weight < 0);
  const exonerations = verdict.reasons.filter((reason) => reason.weight >= 0);
  return [...penalties, ...exonerations];
}

/**
 * Which faces the card lists, in the order a viewer's eye goes.
 *
 * Gating faces first, then by size. A group photograph has thirty faces and a card that
 * listed all of them would bury the one that matters.
 */
export function listedEyes(verdict: IntegrityDto): EyeStateDto[] {
  return [...verdict.eyes]
    .sort((a, b) => {
      if (a.gates !== b.gates) {
        return a.gates ? -1 : 1;
      }
      return b.areaFrac - a.areaFrac;
    })
    .slice(0, 6);
}

/** How one face's eyes read. */
export function eyeLabel(eye: EyeStateDto): string {
  switch (eye.state) {
    case 'closed':
      return eye.intentional ? 'eyes closed - part of the moment' : 'eyes closed';
    case 'squint':
      return 'squinting';
    case 'looking_down':
      return 'looking down';
    case 'occluded':
      return 'eyes hidden';
    default:
      return 'eyes open';
  }
}

/**
 * The style that positions an evidence crop over a preview.
 *
 * Percentages rather than pixels, so the same rectangle addresses a thumbnail, a proxy
 * and a full-resolution view without the card knowing which it is looking at.
 */
export function cropStyle(crop: CropRectDto): Record<string, string> {
  return {
    left: `${(crop.x * 100).toFixed(2)}%`,
    top: `${(crop.y * 100).toFixed(2)}%`,
    width: `${(crop.w * 100).toFixed(2)}%`,
    height: `${(crop.h * 100).toFixed(2)}%`,
  };
}

/**
 * Whether a mark can be dismissed.
 *
 * The three flags that describe something *right* with a photograph cannot: there is
 * nothing to forgive, and offering the button would suggest otherwise.
 */
export function dismissible(flag: string): boolean {
  return !['intentional_motion', 'eyes_closed_ok', 'no_subject_detected'].includes(flag);
}

/** What the dismiss button promises, which is deliberately small. */
export function dismissConsequence(): string {
  return 'This removes the mark and remembers that you disagreed. It does not change whether the photograph is delivered - nothing in this panel does.';
}

/** Per-image technical readout, with the crop behind every claim. */
export function IntegrityCard({ photoId }: { photoId: string }): JSX.Element {
  const [verdict, setVerdict] = useState<IntegrityDto | null>(null);
  const [checked, setChecked] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const found = await api.imageIntegrity(photoId);
      setVerdict(found);
      setChecked(true);
      setError(null);
    } catch (raised) {
      setError(asIpcError(raised).message);
    }
  }, [photoId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const dismiss = useCallback(
    async (flag: string) => {
      try {
        setVerdict(await api.dismissFlag({ photoId, flag }));
        setError(null);
      } catch (raised) {
        setError(asIpcError(raised).message);
      }
    },
    [photoId],
  );

  if (error !== null) {
    return (
      <section className="integrity" aria-label="Technical check">
        <p className="integrity-error">{error}</p>
      </section>
    );
  }

  if (!checked) {
    return (
      <section className="integrity" aria-label="Technical check">
        <p>Reading the technical check…</p>
      </section>
    );
  }

  if (verdict === null) {
    return (
      <section className="integrity" aria-label="Technical check">
        <p className="integrity-unchecked">{notCheckedSentence()}</p>
      </section>
    );
  }

  return (
    <section className="integrity" aria-label="Technical check">
      <h2>{headline(verdict)}</h2>
      <p className="integrity-confidence">{confidenceSentence(verdict)}</p>

      <dl className="integrity-measures">
        <dt>Subject sharpness</dt>
        <dd>{verdict.subjectSharpness.toFixed(2)}</dd>
        <dt>Sharpest of its moment</dt>
        <dd>{verdict.relativeSharpness.toFixed(2)}</dd>
        <dt>Exposure</dt>
        <dd>{exposureSentence(verdict)}</dd>
        <dt>Noise</dt>
        <dd>{noiseSentence(verdict)}</dd>
        <dt>Eyes</dt>
        <dd>{eyeSentence(verdict)}</dd>
      </dl>

      <ul className="integrity-reasons">
        {orderedReasons(verdict).map((reason) => (
          <li
            key={reason.code}
            className={reason.exoneration ? 'integrity-exoneration' : 'integrity-penalty'}
          >
            <span className="integrity-reason-text">{reason.text}</span>
            {reason.evidence !== null && (
              <span
                className="integrity-evidence"
                style={cropStyle(reason.evidence)}
                aria-label="The part of the photograph this is about"
              />
            )}
          </li>
        ))}
      </ul>

      <ul className="integrity-eyes">
        {listedEyes(verdict).map((eye) => (
          <li key={eye.faceId} className={eye.gates ? 'integrity-eye-gates' : 'integrity-eye'}>
            {eyeLabel(eye)}
          </li>
        ))}
      </ul>

      <div className="integrity-dismiss">
        {verdict.flags.filter(dismissible).map((flag) => (
          <button key={flag} type="button" onClick={() => void dismiss(flag)}>
            {`"${flag.replace(/_/g, ' ')}" is wrong`}
          </button>
        ))}
        <p className="integrity-reassurance">{dismissConsequence()}</p>
      </div>
    </section>
  );
}
