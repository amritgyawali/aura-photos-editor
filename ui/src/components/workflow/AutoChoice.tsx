import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * The recommended choice, taken for a photographer who has walked away. ADR-0189.
 *
 * The automatic workflow stops three times for a person: to confirm the cull's best set, to go on
 * to rendering once the edits are saved, and to export. A photographer who imports a wedding and
 * leaves would come back to a run stuck on the first of those. So each stop arms this timer:
 * after one minute with no mouse, keyboard, touch or scroll in the window, it takes the
 * recommended option - the one the primary button already offers. Any activity starts the minute
 * again, and "Let me decide" turns it off for that step, so a person at the screen is never
 * overtaken.
 */

/** One minute without any activity in the window. */
export const AUTO_CHOICE_MS = 60_000;

const ACTIVITY = ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart'] as const;

export type AutoChoice = {
  /** Whole seconds until the choice is taken; null when the timer is not running. */
  secondsLeft: number | null;
  /** Turn the timer off for this step: the person will decide. */
  decline: () => void;
};

/**
 * Take `choose` once, after `ms` without activity, while `armed`. The minute starts again whenever
 * the window sees activity or `armed` becomes true; `step` names the decision, and a new step
 * clears an earlier "Let me decide".
 */
export function useAutoChoice(armed: boolean, choose: () => void, step: string | number = 0, ms = AUTO_CHOICE_MS): AutoChoice {
  const [declined, setDeclined] = useState<string | number | null>(null);
  const [secondsLeft, setSecondsLeft] = useState<number | null>(null);
  const lastActivity = useRef(0);
  const chooseRef = useRef(choose);
  chooseRef.current = choose;
  const live = armed && declined !== step;

  useEffect(() => {
    if (!live) {
      setSecondsLeft(null);
      return undefined;
    }
    lastActivity.current = Date.now();
    setSecondsLeft(Math.ceil(ms / 1000));
    const touched = () => { lastActivity.current = Date.now(); };
    for (const name of ACTIVITY) window.addEventListener(name, touched, { passive: true });
    let fired = false;
    const timer = window.setInterval(() => {
      if (fired) return;
      const left = ms - (Date.now() - lastActivity.current);
      if (left <= 0) {
        fired = true;
        window.clearInterval(timer);
        setSecondsLeft(null);
        chooseRef.current();
        return;
      }
      setSecondsLeft(Math.ceil(left / 1000));
    }, 250);
    return () => {
      window.clearInterval(timer);
      for (const name of ACTIVITY) window.removeEventListener(name, touched);
    };
  }, [live, ms]);

  const decline = useCallback(() => setDeclined(step), [step]);
  return { secondsLeft, decline };
}

/** "0:42". */
export function clock(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/** The line that says what will happen, and when, if nobody chooses. */
export function AutoChoiceNotice({ choice, action }: { choice: AutoChoice; action: string }): JSX.Element | null {
  if (choice.secondsLeft === null) return null;
  return (
    <p className="auto-choice" role="status">
      <span>No choice yet: AURA will {action} in <b>{clock(choice.secondsLeft)}</b>. Any mouse or key activity restarts the minute.</span>
      <button type="button" onClick={choice.decline}>Let me decide</button>
    </p>
  );
}
