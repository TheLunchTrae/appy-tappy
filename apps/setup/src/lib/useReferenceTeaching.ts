// Teaching a reference on the capture screen: its recording and review, the text found on a screenshot, the check of
// its characters, and saving it.

import { type Dispatch, type SetStateAction, useMemo, useState } from 'react';

import type { PinDef, RecordingScope, ReferenceRef, SampleDef, SavedReference } from './bindings.ts';
import { errorText, saveReference, saveReferenceSettings } from './api.ts';
import { boxOf } from './boxes.ts';
import { type Editing, teachingNeeds } from './captureDraft.ts';
import {
  type Built,
  type Draft,
  type ReferenceSaving,
  boxBlocked,
  findBlocked as findBlockedBy,
  readingOf,
  recordBlocked,
  recordedPlace,
  settingsDiffer,
  withoutTeaching,
} from './check.ts';
import { progress } from './confirm.ts';
import { discovery } from './discovery.ts';
import { coverage } from './expected.ts';
import type { Picture } from './pixels.ts';
import { knownCharacters } from './reference.ts';
import type { useBoxes } from './useBoxes.ts';
import { useCharacterCheck } from './useCharacterCheck.ts';
import { useReadingRisks } from './useReadingRisks.ts';
import { useUnlearned } from './useUnlearned.ts';
import { useFound } from './useFound.ts';
import { useRecording } from './useRecording.ts';
import { focusNext, useRecordFocus, useReview } from './useReview.ts';

/** The saved samples of a new reference. */
const NO_SAMPLES: readonly SampleDef[] = [];

/** What the capture screen gives the teaching of its reference. */
interface TeachingSetup {
  /** The saved application the reference belongs to. */
  readonly app: string;
  /** The reference the editor teaches, or null while no editor is open. */
  readonly editing: Editing | null;
  readonly setEditing: Dispatch<SetStateAction<Editing | null>>;
  readonly draft: Draft;
  readonly setDraft: Dispatch<SetStateAction<Draft>>;
  readonly boxes: ReturnType<typeof useBoxes>;
  /** The saved reference the editor adds to, or null for a new one or while no editor is open. */
  readonly reference: ReferenceRef | null;
  /** What saving the reference sends, or null while no editor is open. */
  readonly saving: Built<ReferenceSaving> | null;
  /** The side a recording's region stays at. */
  readonly pin: PinDef | null;
  /** Why nothing can be recorded for want of a window, or null while one is captured. */
  readonly windowless: string | null;
  /** Each screenshot's pixels. */
  readonly full: ReadonlyMap<number, Picture>;
  /** The picture the stage shows, or null while it shows none. */
  readonly shown: Picture | null;
  readonly setSelected: (index: number | null) => void;
  /** Ends picking a color from the picture. */
  readonly stopPicking: () => void;
  /** Hears when a save starts and ends. */
  readonly onSaving: (saving: boolean) => void;
  /**
   * Hears what a save made, and how to leave the editor for the value. What it throws shows as why the reference
   * was not saved.
   */
  readonly onSaved: (saved: SavedReference, close: () => void) => Promise<void>;
}

/**
 * The reference the capture screen teaches while its editor is open: what it learns from, its recording, the review
 * and the check that show in place of the window's picture, and its commands.
 */
export function useReferenceTeaching({
  app,
  editing,
  setEditing,
  draft,
  setDraft,
  boxes,
  reference,
  saving,
  pin,
  windowless,
  full,
  shown,
  setSelected,
  stopPicking,
  onSaving,
  onSaved,
}: TeachingSetup) {
  const recorder = useRecording(editing !== null);
  const recording = recorder.view;
  // Where the next new recording looks for characters. Record more goes on with the recording's own.
  const [recordScope, setRecordScope] = useState<RecordingScope>('place');
  // Why the reference was not saved. It is the editor's own, and goes when the editor closes, so it never shows as
  // the value's.
  const [error, setError] = useState<string | null>(null);
  // The characters what it learns is compared with; none filters no find.
  const needs = teachingNeeds(editing);
  const focus = useRecordFocus();
  const review = useReview(recording, draft, setDraft, reference, needs, focus);
  // What the reference being edited learns from: its saved pictures, its boxes with text, and its kept readings.
  const savedSamples = editing?.existing?.samples;
  const taught = useMemo(
    () => ({
      saved: (savedSamples ?? [])
        .filter((sample) => !draft.dropped.includes(sample.image))
        .map((sample) => sample.text),
      labeled: draft.boxes.map((box) => box.text).filter((text) => text.trim() !== ''),
      recorded: [...draft.recorded.values()],
    }),
    [savedSamples, draft.dropped, draft.boxes, draft.recorded],
  );
  // Every text it learns from already, which tells a find's letters apart from a misread.
  const learnedTexts = useMemo(() => [...taught.saved, ...taught.labeled, ...taught.recorded], [taught]);
  const covered = coverage(needs, knownCharacters(learnedTexts));
  const finder = useFound(draft, boxes.add, reference, learnedTexts, needs);
  const { reviewing } = review;
  const guided = useCharacterCheck(finder, review, recorder, needs, {
    boxes: draft.boxes,
    recorded: draft.recorded,
    full,
    retype: boxes.retype,
    keepReading: (index, text) => {
      setDraft((current) => {
        const recorded = new Map(current.recorded);
        if (text === null) {
          recorded.delete(index);
        } else {
          recorded.set(index, text);
        }
        return { ...current, recorded };
      });
    },
  });
  const taughtCharacters = saving?.ok === true && saving.spec.kind === 'taught' ? saving.spec.characters : null;
  // What may keep it from reading as its samples do.
  const risks = useReadingRisks(taughtCharacters);
  // The saved pictures a save would refuse, which the editor shows so they can be left out.
  const unlearned = useUnlearned(saving?.ok === true ? saving.spec : null, savedSamples ?? NO_SAMPLES);
  // The check, like review, shows in place of the window's picture.
  const checking = guided.guide !== null;
  // The recording review shows. Review is open only on a recording, and one name decides everything review changes
  // on the screen, so the bar, the size, and the sheet never disagree.
  const reviewed = reviewing ? recording : null;
  const checkStep = guided.guide === null ? null : progress(guided.guide, guided.places, needs);

  // Where the reference being taught reads, and the size of the screenshot that is on, which its boxes keep to.
  const reading = readingOf(draft);
  const readingPicture = reading.drawnOn === null ? undefined : full.get(reading.drawnOn);
  const readingSize =
    readingPicture === undefined ? null : { width: readingPicture.width, height: readingPicture.height };

  // Starts a new recording of `from`'s region, or with `more` goes on with the stopped one.
  const record = async (
    more: boolean,
    from: Pick<Draft, 'region' | 'drawnOn'> = { ...draft, ...reading },
  ): Promise<void> => {
    const { region, drawnOn } = from;
    if (region === null || drawnOn === null) {
      return;
    }
    // A reference taught by itself reads at its first box, which the recording is of.
    const readAt = draft.region === null ? (draft.boxes[0]?.id ?? null) : null;
    const scope = more ? (recording?.settings.scope ?? recordScope) : recordScope;
    // Each box is one more example of the text a recording of the whole window looks for.
    const examples = draft.boxes.map((box) => ({ region: box.region, screenshot: box.screenshot }));
    await recorder.start({ app, pin, region, drawnOn, scope, examples }, more, () => {
      review.show(false);
      setSelected(null);
      stopPicking();
      if (!more) {
        review.forget();
        boxes.recorded(readAt);
      }
    });
  };

  // Stops recording and opens the readings to review.
  const stop = async (): Promise<void> => {
    if (await recorder.stop()) {
      review.show(true);
    }
  };

  const discard = async (): Promise<void> => {
    if (await recorder.discard()) {
      review.discarded();
    }
  };

  // The stopped recording's settings, when the draft no longer has them, so Record more can offer to take them back.
  const settings = recording?.settings;
  const settingsChanged =
    recording !== null &&
    !recording.recording &&
    settings !== undefined &&
    settingsDiffer(draft, settings, readingSize);
  const useSettings = (): void => {
    if (settings === undefined) {
      return;
    }
    // A reference taught by itself reads at a box, so that box goes back where the recording read; the value's
    // region is the value's, and stays.
    const place = recordedPlace(draft, settings, boxOf(draft.boxes, boxes.recordedId)?.screenshot ?? null, [...full]);
    if (place !== null) {
      boxes.restore(place, settings.region);
    }
    recorder.clearCommandError();
    // The pressed button gives way to Record more, so focus moves to it.
    focusNext(focus.more);
  };

  // Opens the editor as `editing`, on a saved reference or on a new one read from the value's region.
  const open = (opened: Editing): void => {
    setEditing(opened);
    setDraft(withoutTeaching);
    boxes.select(null);
    stopPicking();
    setError(null);
  };

  // Leaves the editor for the value, forgetting its boxes and recording.
  const close = (): void => {
    setEditing(null);
    setError(null);
    stopPicking();
    boxes.select(null);
    finder.reset();
    guided.forget();
    setDraft(withoutTeaching);
  };

  const save = async (): Promise<void> => {
    if (editing === null || saving === null || !saving.ok) {
      return;
    }
    const { spec } = saving;
    const named = { referenceName: editing.name.trim() };
    onSaving(true);
    try {
      const saved = await (spec.kind === 'taught'
        ? saveReference({ ...named, app, characters: spec.characters })
        : saveReferenceSettings({
            ...named,
            reference: spec.reference,
            dropped: spec.dropped,
            ...(spec.characters === undefined ? {} : { characters: spec.characters }),
          }));
      setError(null);
      await onSaved(saved, close);
    } catch (failed: unknown) {
      setError(errorText(failed));
    } finally {
      onSaving(false);
    }
  };

  // What the recording found, against what the reference needs and knows without it.
  const found = useMemo(
    () =>
      discovery(
        needs,
        knownCharacters([...taught.saved, ...taught.labeled]),
        recording?.readings ?? [],
        review.entries,
        review.checked,
        recording?.shapes.length ?? 0,
      ),
    [needs, taught, recording, review.entries, review.checked],
  );

  return {
    recorder,
    recording,
    recordScope,
    setRecordScope,
    /** Why the reference was not saved, or null. */
    error,
    needs,
    focus,
    review,
    taught,
    covered,
    risks,
    unlearned,
    finder,
    guided,
    checking,
    reviewed,
    /** Whether review or the check shows in place of the window's picture. */
    sheeted: reviewed !== null || checking,
    checkStep,
    recordError: recorder.commandError ?? review.error ?? recorder.pollError,
    /** Why it cannot record, or null when it can or no editor is open. */
    recordBlocked: editing === null ? null : (windowless ?? recordBlocked(draft)),
    /** Why Find more like it cannot search from the selected box, or null. */
    findBlocked: findBlockedBy(boxes.selected),
    /** Why no box can be drawn on the picture shown, or null. */
    boxBlock: editing === null ? null : boxBlocked(readingSize, shown),
    /** The name the editor's reference shows under. */
    name: editing === null || editing.name.trim() === '' ? 'New reference' : editing.name.trim(),
    settingsChanged,
    useSettings,
    found,
    record,
    stop,
    discard,
    open,
    close,
    save,
  };
}

/** A reference being taught, as the capture screen and its inspector use it. */
export type Teaching = ReturnType<typeof useReferenceTeaching>;
