// Setting up a bar fill check: detecting the bar's colors, and what the colors measure on the screenshots.

import { type Dispatch, type SetStateAction, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { BarReading } from './bindings.ts';
import { detectBar, errorText, measureBar } from './api.ts';
import type { Draft } from './check.ts';
import {
  type BarColors,
  type DetectEvent,
  type DetectKey,
  type Detecting,
  type Measured,
  detectStep,
  leftOutText,
  measureKey,
  measuredFor,
} from './detect.ts';
import { background } from './repeat.ts';

/** How long a measure waits after the last change before it is made, so a dragged region asks once. */
const MEASURE_DELAY_MS = 150;

/** What the capture screen shows of a bar fill check being set up. */
export interface BarSetup {
  readonly detecting: Detecting;
  /** Whether there is a region on a screenshot to look for a bar in. */
  readonly canDetect: boolean;
  readonly detect: () => void;
  /** Takes the colors found in place of the ones picked by hand. */
  readonly use: () => void;
  /** Keeps the colors picked by hand. */
  readonly keep: () => void;
  /** The colors measured: the ones found while the user is asked about them, or else the draft's. */
  readonly colors: BarColors | null;
  /** What each screenshot reads with `colors`, or null until that is known. */
  readonly readings: readonly BarReading[] | null;
  /** That older screenshots are not among `readings`, or null when all of the window's size are. */
  readonly leftOut: string | null;
  readonly measureError: string | null;
  readonly swap: () => void;
}

/**
 * Detects the colors of the bar in `draft`'s region when asked, and measures the colors on the session's `shots`
 * screenshots while `active`.
 */
export function useBarSetup(
  draft: Draft,
  setDraft: Dispatch<SetStateAction<Draft>>,
  shots: number,
  active: boolean,
): BarSetup {
  const { kind, region, drawnOn, filled, empty } = draft;
  const key = useMemo(
    (): DetectKey | null =>
      kind === 'barFill' && region !== null && drawnOn !== null ? { region, drawnOn, shots } : null,
    [kind, region, drawnOn, shots],
  );
  const [detecting, setDetecting] = useState<Detecting>({ kind: 'idle' });
  // What an answer that arrives later meets: the detection as it stands and the draft's colors.
  const latest = useRef({ detecting, picked: { filled, empty } });
  latest.current.picked = { filled, empty };
  const send = useCallback(
    (event: DetectEvent): void => {
      const next = detectStep(latest.current.detecting, event);
      latest.current.detecting = next.state;
      setDetecting(next.state);
      const colors = next.use;
      if (colors !== null) {
        setDraft((current) => ({ ...current, ...colors }));
      }
    },
    [setDraft],
  );
  useEffect(() => {
    send({ kind: 'inputs', key });
  }, [key, send]);

  const detect = (): void => {
    if (key === null) {
      return;
    }
    send({ kind: 'ask', key });
    const answer = async (): Promise<void> => {
      try {
        const detection = await detectBar(key.region, key.drawnOn);
        send({ kind: 'answer', key, detection, picked: latest.current.picked });
      } catch (error: unknown) {
        send({ kind: 'fail', key, message: errorText(error) });
      }
    };
    background(answer());
  };

  const offered = detecting.kind === 'found' && detecting.asking ? detecting.proposal : null;
  const colors = useMemo((): BarColors | null => {
    if (offered !== null) {
      return { filled: offered.filled, empty: offered.empty };
    }
    return filled === null || empty === null ? null : { filled, empty };
  }, [offered, filled, empty]);
  const [made, setMade] = useState<Measured | null>(null);
  const wanted = active && key !== null && colors !== null ? measureKey(key, colors) : null;
  useEffect(() => {
    if (wanted === null || key === null || colors === null) {
      return undefined;
    }
    let current = true;
    const timer = setTimeout(() => {
      measureBar(key.region, key.drawnOn, colors.filled, colors.empty)
        .then((measure) => {
          if (current) {
            setMade({ key: wanted, measure, error: null });
          }
        })
        .catch((error: unknown) => {
          if (current) {
            setMade({ key: wanted, measure: null, error: errorText(error) });
          }
        });
    }, MEASURE_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
    // `wanted` spells out the key and the colors.
  }, [wanted]);
  const measure = measuredFor(made, wanted)?.measure ?? null;
  return {
    detecting,
    canDetect: key !== null,
    detect,
    use: () => {
      send({ kind: 'use' });
    },
    keep: () => {
      send({ kind: 'keep' });
    },
    colors,
    readings: measure?.readings ?? null,
    leftOut: measure === null ? null : leftOutText(measure.leftOut),
    measureError: measuredFor(made, wanted)?.error ?? null,
    swap: () => {
      setDraft((current) => ({ ...current, filled: current.empty, empty: current.filled }));
    },
  };
}
