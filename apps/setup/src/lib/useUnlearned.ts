// The saved pictures a reference being edited can't learn with the text color and tolerance it has now, which a save
// would refuse. The editor shows each with its picture, since no box and no screenshot shows it, so the user can
// leave it out.

import { useEffect, useRef, useState } from 'react';

import type { ReferenceRef, SampleDef, SavingKind, UnlearnedSample } from './bindings.ts';
import { samplePicture, unlearnedSamples } from './api.ts';
import type { ReferenceSaving } from './check.ts';
import type { Color, Picture } from './pixels.ts';
import { ask } from './request.ts';
import { background } from './repeat.ts';

/** How long the settings must rest before the pictures are judged, so a dragged tolerance asks once. */
const JUDGE_DELAY_MS = 250;

/** The widest a saved picture is fetched, which the row's crop scales down. */
const CROP_WIDTH = 240;

/** What the setup app is asked about a saved reference's pictures. */
interface Asked {
  readonly reference: ReferenceRef;
  readonly ink: Color | null;
  readonly tolerance: number;
  readonly dropped: readonly string[];
  readonly saving: SavingKind;
}

/** What `saving` asks about its saved reference's pictures, or null when it saves a new reference or nothing. */
export function unlearnedAsk(saving: ReferenceSaving | null): Asked | null {
  if (saving === null) {
    return null;
  }
  if (saving.kind === 'settings') {
    const { reference, ink, tolerance, dropped } = saving;
    return { reference, ink, tolerance, dropped, saving: 'settings' };
  }
  const { reference, ink, tolerance, dropped } = saving.characters;
  return reference === null ? null : { reference, ink: ink ?? null, tolerance, dropped, saving: 'taught' };
}

/**
 * A name for what `asked` asks about the saved pictures `samples`, which stays while nothing the answer depends on
 * changes, or null when it asks nothing. The boxes and readings a reference adds are not in it: its saved pictures
 * are learned before them.
 */
export function unlearnedKey(asked: Asked | null, samples: readonly SampleDef[]): string | null {
  return asked === null ? null : JSON.stringify([asked, samples.map((sample) => sample.image)]);
}

/** An answer of the setup app, and the key it was asked under. */
interface Answer {
  readonly key: string;
  readonly samples: readonly UnlearnedSample[];
}

const NONE: readonly UnlearnedSample[] = [];

const NO_PICTURES: ReadonlyMap<string, Picture> = new Map();

/**
 * The pictures to show while the editor asks under `wanted`: those of `answer` when it was asked under that key, and
 * none otherwise, so an answer for a text color, a tolerance, or saved pictures since changed is dropped.
 */
export function unlearnedArrived(answer: Answer, wanted: string | null): readonly UnlearnedSample[] {
  return answer.key === wanted ? answer.samples : NONE;
}

/** The saved pictures a reference can't learn, and the pixels of those that have arrived, by file name. */
export interface Unlearned {
  readonly samples: readonly UnlearnedSample[];
  readonly pictures: ReadonlyMap<string, Picture>;
}

/**
 * The saved pictures, of `samples`, that the reference `saving` saves can't learn, as the setup app answers for
 * them, with their pixels; none while `saving` is null, it saves a new reference, or the answer has not arrived. It
 * asks once `ready`, and again once the text color, the tolerance, the pictures left out, or the saved pictures
 * changed and then rested. While it is not `ready`, as while a tolerance is fitted to a box being typed, the answer
 * for the settings it still has stays. An ask that fails answers none, and saving says what is wrong.
 */
export function useUnlearned(saving: ReferenceSaving | null, samples: readonly SampleDef[], ready: boolean): Unlearned {
  const asked = unlearnedAsk(saving);
  const key = unlearnedKey(asked, samples);
  const latest = useRef(asked);
  latest.current = asked;
  const [answer, setAnswer] = useState<Answer>({ key: '', samples: NONE });
  const [crops, setCrops] = useState<{ readonly key: string; readonly pictures: ReadonlyMap<string, Picture> }>({
    key: '',
    pictures: new Map(),
  });
  useEffect(() => {
    const now = latest.current;
    if (now === null || key === null || !ready) {
      return undefined;
    }
    return ask(
      () => unlearnedSamples(now.reference, now.ink, now.tolerance, now.dropped, now.saving),
      (unlearned) => {
        setAnswer({ key, samples: unlearned });
        for (const { image } of unlearned) {
          background(
            samplePicture(now.reference, image, CROP_WIDTH).then((picture) => {
              if (picture === null) {
                return;
              }
              setCrops((current) => {
                const pictures = current.key === key ? new Map(current.pictures) : new Map<string, Picture>();
                return { key, pictures: pictures.set(image, picture) };
              });
            }),
          );
        }
      },
      (error) => {
        console.error(error);
      },
      JUDGE_DELAY_MS,
    );
  }, [key, ready]);
  return {
    samples: unlearnedArrived(answer, key),
    pictures: crops.key === key ? crops.pictures : NO_PICTURES,
  };
}
