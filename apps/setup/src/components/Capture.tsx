import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { ReferenceDef, ReferenceRef, ValueDef } from '../lib/bindings.ts';
import { editValue, errorText, listDrafts, placeValue, saveFix, saveReference, saveValue } from '../lib/api.ts';
import { LiveCanvas, PictureCanvas } from './canvas.tsx';
import {
  type Built,
  type Draft,
  buildCharacters,
  buildSpec,
  CHECK_KINDS,
  EMPTY_DRAFT,
  fixDraft,
  fromReference,
  KIND_NAMES,
  newestOfSize,
  findBlocked as findBlockedBy,
  recordBlocked,
  regionScreenshot,
  settingsDiffer,
  toReference,
  withFixRegion,
  withKind,
  withState,
} from '../lib/check.ts';
import { discovery } from '../lib/discovery.ts';
import { agreedFixes, editStart, keepsStates, placedOn, saveable } from '../lib/edit.ts';
import { TOO_MANY, coverage, startingExpected, tooMany } from '../lib/expected.ts';
import { knownCharacters, readers } from '../lib/reference.ts';
import { BarDetect, BarReadings } from './BarFields.tsx';
import { type ColorTarget, ColorFields, SelectFields } from './CheckFields.tsx';
import { ReadingBox, RegionFields } from './Fields.tsx';
import { FoundMarks, FoundPanel } from './Found.tsx';
import { Question, useQuestion } from './Question.tsx';
import { ReferenceEditor } from './ReferenceEditor.tsx';
import { ReferencePicker } from './ReferencePicker.tsx';
import type { Choice } from './Picker.tsx';
import { colorAt } from '../lib/pixels.ts';
import { RecordPanel, ReviewSheet } from './Recording.tsx';
import { LookPanel } from './LookRecording.tsx';
import { listedKept, lookBlocked, lookTolerance, removableRecorded, removeBlocked } from '../lib/looks.ts';
import { clock } from '../lib/review.ts';
import type { Point, Size } from '../lib/region.ts';
import { RegionSurface } from './RegionSurface.tsx';
import { ZoomView } from './ZoomView.tsx';
import { PinPicker } from './PinPicker.tsx';
import { DeleteDraft } from './Drafts.tsx';
import { PinGuides } from './PinGuides.tsx';
import { MovingContentField } from './MovingContentField.tsx';
import { Hint } from './Hint.tsx';
import { MatchOverlay, MatchPreviewField, SelectedPixelsField } from './MatchPreview.tsx';
import { AREA_DRAW_HINT, fixNote, isReadingArea, movingContent, regionLabel } from '../lib/moving.ts';
import { pinChoiceAfterRegion, pinOfView, pinView } from '../lib/pin.ts';
import { FIX_HINT, READ_WITH_HINT, sameSize, sizeText } from '../lib/sizes.ts';
import type { SelectTool } from '../lib/selection.ts';
import { StatesField } from './StatesField.tsx';
import { EditIntro, EditOutcome, SavedStates } from './ValueEdit.tsx';
import { CHECK_HINT, DRAFT_HINT } from '../lib/hints.ts';
import {
  type CaptureKept,
  type CaptureSetup,
  type Editing,
  type Step,
  captureMissing,
  restoreSetup,
  storeSetup,
} from '../lib/captureDraft.ts';
import { type ReportDraft, type Resumed, draftsOf, isKept, keep, useDraft } from '../lib/drafts.ts';
import { background } from '../lib/repeat.ts';
import { type Made, savedNote } from '../lib/flow.ts';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';
import { useAppFile } from '../lib/useAppFile.ts';
import { useBarSetup } from '../lib/useBarSetup.ts';
import { useFound } from '../lib/useFound.ts';
import { trialOf, useLiveCheck } from '../lib/useLiveCheck.ts';
import { useLookRecording } from '../lib/useLookRecording.ts';
import { useMatchPreview, useSelectedPreview } from '../lib/useMatchPreview.ts';
import { useRecording } from '../lib/useRecording.ts';
import { useReview } from '../lib/useReview.ts';
import { useScreenshotShortcut } from '../lib/useScreenshotShortcut.ts';
import { useScreenshots } from '../lib/useScreenshots.ts';
import { useStateSelections } from '../lib/useStateSelections.ts';
import { useValueEdit } from '../lib/useValueEdit.ts';

/** The widest live frame asked for, which keeps a large window's preview cheap. */
const LIVE_MAX_WIDTH = 1600;

function viewHint(
  recording: 'readings' | 'looks' | null,
  selected: number | null,
  shotCount: number,
  picking: boolean,
  liveError: string | null,
  shortcut: string,
  tool: SelectTool,
  area: boolean,
  otherSize: boolean,
): string {
  if (selected === null && liveError !== null) {
    return liveError;
  }
  if (recording !== null && selected === null) {
    return recording === 'readings'
      ? 'Recording. Use the application; a reading is kept for each new shape.'
      : 'Recording. Use the application; a screenshot is kept of each different look.';
  }
  if (selected === null) {
    return shotCount === 0
      ? `Take a screenshot (${shortcut}) when the window shows what to read.`
      : 'Select a screenshot to draw on it.';
  }
  if (picking) {
    return 'Click the color to pick it.';
  }
  if (otherSize) {
    return 'The region shows on screenshots of the size the value was drawn at.';
  }
  if (tool === 'snap') {
    return 'Drag a rough box around the thing to snap to it.';
  }
  if (tool === 'click') {
    return 'Click the thing to select it.';
  }
  return area ? AREA_DRAW_HINT : 'Drag to draw a region. Drag inside it to move it, or drag a corner to resize it.';
}

/**
 * What the capture screen makes: a value; a reference, new or saved, on the References page; a fix, a saved value
 * drawn again at the window's size now; or an edit, a saved value changed as it was first drawn.
 */
export type CaptureMode =
  | { readonly kind: 'value' }
  | { readonly kind: 'reference'; readonly existing: ReferenceDef | null }
  | { readonly kind: 'fix'; readonly value: ValueDef }
  | { readonly kind: 'edit'; readonly value: ValueDef };

/** The name a draft of the capture lists under: what was typed, or else what it makes or fixes. */
function draftName(mode: Exclude<CaptureMode, { readonly kind: 'edit' }>, setup: CaptureSetup): string {
  switch (mode.kind) {
    case 'fix':
      return mode.value.name;
    case 'reference': {
      const typed = setup.editing?.name.trim() ?? '';
      return typed === '' ? (mode.existing?.name ?? 'New reference') : typed;
    }
    case 'value':
      return setup.valueName.trim() === '' ? 'New value' : setup.valueName.trim();
  }
}

/** A draft the capture finishes: the draft, and which of its screenshots are this session's again, by index. */
export interface Restored {
  readonly resumed: Resumed;
  /** Null once the window changed, which starts the capture again without what the draft kept. */
  readonly screenshots: readonly number[] | null;
}

/** What the capture starts with: what `restored` kept, or else what `mode` makes. */
function startSetup(mode: CaptureMode, restored: Restored | null): CaptureSetup {
  const kept = restored?.screenshots === null ? null : (restored?.resumed.def.state ?? null);
  if (kept !== null && mode.kind !== 'edit' && isKept<CaptureKept>(kept, mode.kind)) {
    const setup = restoreSetup(kept.state);
    // A reference's draft changes the reference as it is saved now.
    return mode.kind === 'reference' && setup.editing !== null
      ? { ...setup, editing: { ...setup.editing, existing: mode.existing } }
      : setup;
  }
  const editing: Editing | null =
    mode.kind === 'reference'
      ? {
          existing: mode.existing,
          name: mode.existing?.name ?? '',
          expects: startingExpected(mode.existing, null),
          valueRegion: { region: null, drawnOn: null },
        }
      : null;
  if (mode.kind === 'edit') {
    return { step: 'where', editing, recordedShots: [], ...editStart(mode.value) };
  }
  return {
    valueName: '',
    step: 'where',
    mayMove: false,
    editing,
    recordedShots: [],
    draft:
      mode.kind === 'fix'
        ? fixDraft(mode.value.screen.check)
        : editing === null
          ? EMPTY_DRAFT
          : toReference(EMPTY_DRAFT, editing.existing),
  };
}

interface Props {
  readonly choice: Choice;
  readonly mode: CaptureMode;
  /** The draft the capture finishes, or null. */
  readonly restored: Restored | null;
  /** The way to the capture's flow, for its breadcrumb: its page, then each flow that waits under it. */
  readonly trail: readonly string[];
  readonly onCancel: () => void;
  readonly onChangeWindow: () => void;
  /** Hears what the flow saved: where a value is, or the application of a reference. */
  readonly onSaved: (made: Made) => void;
  /** Closes the flow once its draft is saved or deleted. */
  readonly onDrafted: () => void;
  /** Hears what leaving the capture would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the capture's draft. */
  readonly onDraft: ReportDraft;
}

export function Capture({
  choice,
  mode,
  restored,
  trail,
  onCancel,
  onChangeWindow,
  onSaved,
  onDrafted,
  onHeld,
  onDraft,
}: Props) {
  // What the capture opens with: what its draft kept, or else what its mode makes.
  const [opening] = useState(() => {
    const setup = startSetup(mode, restored);
    const recorded = new Set(setup.recordedShots);
    return {
      setup,
      shots: (restored?.screenshots ?? []).map((index) => ({ index, recorded: recorded.has(index) })),
    };
  });
  // What the capture shows before the user does anything, which leaving it loses nothing of. A fix and an edit
  // open on a screenshot the capture takes itself, and start from there.
  const [start, setStart] = useState(() => ({
    screenshots: opening.shots.length,
    recording: false,
    valueName: opening.setup.valueName,
    mayMove: opening.setup.mayMove,
    editing: opening.setup.editing,
    draft: opening.setup.draft,
  }));
  const [draft, setDraft] = useState<Draft>(start.draft);
  // The saved application the value or reference belongs to.
  const appId = choice.app;
  const appFile = useAppFile(appId);
  const { file } = appFile;
  const [step, setStep] = useState<Step>(opening.setup.step);
  const [editing, setEditing] = useState<Editing | null>(start.editing);
  const [picking, setPicking] = useState<ColorTarget | null>(null);
  const [tool, setTool] = useState<SelectTool>('box');
  const [valueName, setValueName] = useState(start.valueName);
  const fixing = mode.kind === 'fix' ? mode.value : null;
  // The saved value an edit changes as it was first drawn.
  const edited = mode.kind === 'edit' ? mode.value : null;
  // Whether the user said the value's content may move around inside the window.
  const [mayMove, setMayMove] = useState(start.mayMove);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const screenshots = useScreenshots(opening.shots);
  const { shots, full, selected, setSelected, position } = screenshots;
  const shortcut = useScreenshotShortcut(screenshots.take);
  const recorder = useRecording();
  const recording = recorder.view;
  const looks = useLookRecording(screenshots.adopt);
  // The screenshots the recording the user just stopped kept, until the first that the strip lists shows.
  const [toReview, setToReview] = useState<readonly number[] | null>(null);
  useEffect(() => {
    const first = toReview === null ? undefined : listedKept(toReview, shots)[0];
    if (first !== undefined) {
      setSelected(first);
      setToReview(null);
    }
  }, [toReview, shots, setSelected]);
  // The recording that runs now, of readings or of looks: the owner records one thing at a time.
  const recordingNow = recording?.recording === true ? recording : looks.view?.recording === true ? looks.view : null;

  const setup: CaptureSetup = {
    valueName,
    step,
    mayMove,
    editing,
    draft,
    recordedShots: shots.filter((shot) => shot.recorded).map((shot) => shot.index),
  };
  // The application's references that have a draft of changes. Drafts change only in flows, and this is the open
  // one, so reading them once is enough; when they cannot be read, every reference can be edited.
  const [drafted, setDrafted] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    let live = true;
    background(
      listDrafts()
        .then((drafts) => {
          if (live) {
            setDrafted(
              new Set(
                draftsOf(drafts, 'reference').flatMap(({ def }) =>
                  def.app === appId && def.changes !== null ? [def.changes] : [],
                ),
              ),
            );
          }
        })
        .catch(() => {
          // Without the drafts, nothing is held back.
        }),
    );
    return () => {
      live = false;
    };
  }, [appId]);
  const [leavingWindow, setLeavingWindow] = useState(false);
  const leaveQuestion = useQuestion<HTMLButtonElement>(leavingWindow);

  // The saved reference the editor adds to, or null for a new one or while no editor is open.
  const existingId = editing?.existing?.id ?? null;
  const editingRef = useMemo(
    (): ReferenceRef | null => (existingId === null ? null : { app: appId, id: existingId }),
    [existingId, appId],
  );
  const isEditing = editing !== null;
  const built: Built = useMemo(
    () => (isEditing ? buildCharacters(draft, editingRef) : buildSpec(draft, appId)),
    [isEditing, draft, editingRef, appId],
  );
  const missing = captureMissing(setup, built, fixing !== null);
  // An edit changes a saved value in place and is never a draft.
  const drafting = useDraft(
    restored?.resumed ?? null,
    mode.kind === 'edit'
      ? null
      : () => ({
          kind: mode.kind,
          app: appId,
          changes: mode.kind === 'fix' ? mode.value.id : mode.kind === 'reference' ? (mode.existing?.id ?? null) : null,
          name: draftName(mode, setup),
          missing: [...missing],
          pictures: [],
          state: keep(mode.kind, storeSetup(setup)),
        }),
    true,
    onDraft,
    onDrafted,
  );
  const busy = saving || drafting.saving;
  const holds = held(busy, start, {
    screenshots: shots.length,
    recording: recording !== null,
    valueName,
    mayMove,
    editing,
    draft,
  });
  useReportHeld(onHeld, holds);
  // What belongs to a recording goes with it: the readings one kept, and the one that runs or waits for review.
  const losesRecording = recording !== null || draft.recorded.size > 0 || looks.view?.recording === true;
  const draftButton =
    mode.kind === 'edit' ? null : (
      <>
        {restored === null ? null : (
          <DeleteDraft
            changes={mode.kind === 'fix' || (mode.kind === 'reference' && mode.existing !== null)}
            disabled={busy}
            onDelete={drafting.discard}
          />
        )}
        <span className="with-hint">
          <button
            type="button"
            className="btn"
            disabled={holds !== 'unsaved'}
            aria-describedby="hint-draft"
            onClick={() => {
              background(drafting.saveAndClose());
            }}
          >
            Save as draft
          </button>
          <Hint topic="Save as draft" id="hint-draft">
            {DRAFT_HINT}
            {losesRecording
              ? ' A draft keeps the screenshots, those a recording kept too, but not a recording that runs or the readings kept from one.'
              : ''}
          </Hint>
        </span>
      </>
    );
  const draftError =
    drafting.error === null ? null : (
      <p className="warn" role="alert">
        {drafting.error}
      </p>
    );
  const selection = useStateSelections(draft, setDraft, selected);
  const selecting = draft.kind === 'state' ? tool : 'box';
  const bar = useBarSetup(
    draft,
    setDraft,
    shots.map((each) => each.index),
    step === 'how' && editing === null && fixing === null,
  );
  const review = useReview(recording, draft, setDraft, editingRef);
  // Every text the reference being edited learns from already, which tells a find's letters apart from a misread.
  const savedSamples = editing?.existing?.samples;
  const learnedTexts = useMemo(
    () => [
      ...(savedSamples ?? []).map((sample) => sample.text),
      ...[...draft.labels.values()].filter((text) => text.trim() !== ''),
      ...draft.recorded.values(),
      ...draft.found.map((label) => label.text),
    ],
    [savedSamples, draft.labels, draft.recorded, draft.found],
  );
  const finder = useFound(draft, setDraft, editingRef, learnedTexts);
  const { reviewing } = review;
  const recordError = recorder.commandError ?? review.error ?? recorder.pollError;

  const [liveSize, setLiveSize] = useState<Size | null>(null);
  const [liveError, setLiveError] = useState<string | null>(null);
  const onFrame = useCallback((width: number, height: number): void => {
    setLiveError(null);
    setLiveSize((current) => (current?.width === width && current.height === height ? current : { width, height }));
  }, []);
  const onLiveError = useCallback((error: unknown): void => {
    setLiveError(errorText(error));
  }, []);

  // A fix opens on a screenshot of the window as it is now, with the region where the value would be read there.
  // An edit opens on one too, which its region sits on when the window has the size the value was drawn at. A
  // draft's fix already has its screenshots and region.
  const fixShot = useRef<'none' | 'taken' | 'placed'>(opening.shots.length > 0 ? 'placed' : 'none');
  const take = screenshots.take;
  const opened = fixing ?? edited;
  useEffect(() => {
    if (opened === null || liveSize === null || fixShot.current !== 'none') {
      return;
    }
    fixShot.current = 'taken';
    background(take());
  }, [opened, liveSize, take]);
  const firstShot = shots[0]?.index ?? null;
  const firstPicture = firstShot === null ? undefined : full.get(firstShot);
  useEffect(() => {
    if (opened === null || firstShot === null || firstPicture === undefined || fixShot.current === 'placed') {
      return;
    }
    fixShot.current = 'placed';
    const size = { width: firstPicture.width, height: firstPicture.height };
    const place = async (): Promise<void> => {
      try {
        const region = fixing === null ? null : await placeValue(appId, fixing.id, size.width, size.height);
        const placed = (current: Draft): Draft =>
          edited === null
            ? withFixRegion(current, region, firstShot)
            : placedOn(current, edited.screen, firstShot, size);
        setDraft(placed);
        setStart((began) => ({ ...began, screenshots: 1, draft: placed(began.draft) }));
      } catch (error: unknown) {
        setSaveError(errorText(error));
      }
    };
    background(place());
  }, [opened, fixing, edited, firstShot, firstPicture, appId]);

  const shownOn = regionScreenshot(selected, draft.drawnOn, full, shots.at(-1)?.index ?? null);
  const native = shownOn === null ? undefined : full.get(shownOn);
  // The window size the region's pixels refer to.
  const drawnPicture = draft.drawnOn === null ? undefined : full.get(draft.drawnOn);
  const drawnSize = drawnPicture === undefined ? null : { width: drawnPicture.width, height: drawnPicture.height };
  const shownPicture = selected === null ? null : (full.get(selected) ?? null);
  const layoutSize: Size | null = selected === null ? liveSize : shownPicture;

  const pick = (point: Point): void => {
    if (picking === null || shownPicture === null) {
      return;
    }
    const color = colorAt(shownPicture, point.x, point.y);
    if (color !== null) {
      setDraft((current) => ({ ...current, [picking]: color }));
      setPicking(null);
    }
  };

  // An edit's region keeps to the size the value was drawn at while no screenshot of that size holds it.
  const staysAt = pinView(draft.pin, draft.region, drawnSize ?? edited?.screen.windowSize ?? null);
  const pin = pinOfView(staysAt);
  // The value's region is a reading area while its content may move and its check reads it wherever it is. A fix
  // draws the kind of region its value saved.
  const moving = movingContent(draft.kind, fixing === null ? mayMove : fixing.screen.readingArea === true);
  const area = isReadingArea(moving) && editing === null;
  // A fix keeps its value's side, so only a new value and an edit need one to save.
  const hasSide = fixing !== null || pin !== null;

  const edit = useValueEdit({
    value: edited,
    open: editing === null,
    app: appId,
    draft,
    setDraft,
    name: valueName,
    pin,
    readingArea: isReadingArea(moving),
    sizes: full,
  });
  const trial = useMemo(() => trialOf(built, edit.built), [built, edit.built]);
  const check = useLiveCheck(trial, edit.again);
  const dropFixes = agreedFixes(check.preview, edit.lost);
  // The size of the screenshot the region would show over, when an edited value was drawn at another: its region
  // is in the pixels of the size it was drawn at, so it shows and moves on screenshots of that size only.
  const otherSize =
    edited !== null && editing === null && native !== undefined && !sameSize(native, edited.screen.windowSize)
      ? native
      : null;
  // The preview is for the color fields the inspector shows now.
  const colorFields = step === 'how' && fixing === null && (draft.kind === 'barFill' || draft.kind === 'color');
  const preview = useMatchPreview(
    editing !== null ? 'ink' : colorFields ? draft.kind : null,
    draft,
    selected,
    shownPicture,
    // An edited value's region is in the pixels of the size it was drawn at, and shows on no other.
    otherSize === null || edited === null
      ? null
      : `Select a screenshot at ${sizeText(edited.screen.windowSize)} to see the matching pixels of its region.`,
  );
  const previewField = <MatchPreviewField preview={preview} region={draft.region} />;
  // An edited value's region sits on a screenshot of the size it was drawn at, and a bar is looked for on those.
  const detectBlocked =
    edited !== null && draft.drawnOn === null
      ? `Take a screenshot with the window at ${sizeText(edited.screen.windowSize)} to detect the bar: its region was drawn at that size.`
      : null;
  // Color match is in the inspector while a state's thing is selected with Snap or Click.
  const colorMatched = editing === null && step === 'where' && draft.kind === 'state' && tool !== 'box';
  const selectedPreview = useSelectedPreview(
    colorMatched,
    draft,
    selection.selections,
    selected,
    selection.error !== null,
  );

  // What Save sends, or null while something is missing.
  const sends = (): (() => Promise<Made>) | null => {
    const changes = edit.built;
    if (changes !== null) {
      return changes.ok ? async () => editValue(changes.spec, dropFixes) : null;
    }
    const screenshot = draft.drawnOn;
    if (!built.ok || screenshot === null) {
      return null;
    }
    const { spec } = built;
    if (fixing !== null) {
      return async () => saveFix({ app: choice.app, value: fixing.id, check: spec, screenshot });
    }
    // A new value saves only with a side.
    if (pin === null) {
      return null;
    }
    const readingArea = isReadingArea(moving);
    return async () =>
      saveValue({ app: choice.app, valueName: valueName.trim(), check: spec, screenshot, pin, readingArea });
  };

  const save = async (): Promise<void> => {
    const send = sends();
    if (send === null) {
      return;
    }
    setSaving(true);
    try {
      const made = await send();
      await drafting.finished();
      onSaved(made);
    } catch (error: unknown) {
      setSaveError(errorText(error));
      edit.refused();
    } finally {
      setSaving(false);
    }
  };

  const recordingBlocked = editing === null ? null : recordBlocked(draft);
  const findBlocked = findBlockedBy(draft, drawnSize, shownPicture);

  // Starts a new recording of `from`'s region, or with `more` goes on with the stopped one.
  const record = async (
    more: boolean,
    from: Pick<Draft, 'region' | 'ink' | 'tolerance' | 'drawnOn'> = draft,
  ): Promise<void> => {
    const { region, ink, tolerance, drawnOn } = from;
    if (region === null || ink === null || drawnOn === null) {
      return;
    }
    await recorder.start({ region, ink, tolerance, drawnOn }, more, () => {
      review.show(false);
      setSelected(null);
      setPicking(null);
      if (!more) {
        review.forget();
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

  // The stopped recording's settings, when the draft no longer has them, so
  // Record more can offer to take them back.
  const settings = recording?.settings;
  const settingsChanged =
    recording !== null && !recording.recording && settings !== undefined && settingsDiffer(draft, settings, drawnSize);
  const useSettings = (): void => {
    if (settings === undefined) {
      return;
    }
    // The region goes back on a screenshot of the size it was recorded at.
    const sameSize =
      drawnSize?.width === settings.windowSize.width && drawnSize.height === settings.windowSize.height
        ? draft.drawnOn
        : newestOfSize([...full], settings.windowSize);
    setDraft((current) => ({
      ...current,
      region: settings.region,
      drawnOn: sameSize ?? current.drawnOn,
      ink: settings.ink,
      tolerance: settings.tolerance,
    }));
    recorder.clearCommandError();
    // The pressed button gives way to Record more, so focus moves to it.
    requestAnimationFrame(() => {
      document.querySelector<HTMLButtonElement>('[data-record-more]')?.focus();
    });
  };

  // Records the looks the value's region shows, in place of any recording.
  const recordLooks = async (): Promise<void> => {
    const { region, drawnOn } = draft;
    if (region === null || drawnOn === null) {
      return;
    }
    await looks.start({ region, tolerance: lookTolerance(draft), drawnOn }, () => {
      setSelected(null);
      setPicking(null);
      setToReview(null);
    });
  };

  // Stops recording looks, to review what it kept in the strip.
  const stopLooks = async (): Promise<void> => {
    const stopped = await looks.stop();
    if (stopped !== null && stopped.kept.length > 0) {
      setToReview(stopped.kept);
    }
  };

  // Opens the reference editor on `existing`, or on a new reference read
  // from the value's region, which starts with what `like` expects: the
  // reference it is learned again for at this size, if any. The editor
  // records readings, and the owner records one thing at a time, so a
  // recording of looks goes first.
  const openEditor = async (existing: ReferenceDef | null, like: ReferenceDef | null): Promise<void> => {
    setEditing({
      existing,
      name: existing?.name ?? '',
      expects: startingExpected(existing, like),
      valueRegion: { region: draft.region, drawnOn: draft.drawnOn },
    });
    setDraft((current) => toReference(current, existing));
    setPicking(null);
    setSaveError(null);
    await looks.discard();
  };

  // Opens the reference editor on the picked reference, or on a new one, to
  // record the value's region. A saved reference has its characters' color,
  // so recording starts at once; a new one has none, so picking it is next.
  const recordReadings = async (): Promise<void> => {
    const existing = file?.references.find((reference) => reference.id === draft.reference) ?? null;
    const { region, drawnOn } = draft;
    await openEditor(existing, null);
    if (region === null || drawnOn === null) {
      return;
    }
    if (existing === null) {
      setSelected(drawnOn);
      setPicking('ink');
      requestAnimationFrame(() => {
        document.getElementById('record-title')?.focus();
      });
      return;
    }
    await record(false, { region, drawnOn, ink: existing.ink, tolerance: existing.tolerance });
  };

  // Leaves the reference editor for the value, forgetting its labels and
  // recording and putting the value's region back.
  const closeEditor = (): void => {
    if (recording !== null) {
      background(discard());
    }
    const valueRegion = editing?.valueRegion ?? { region: null, drawnOn: null };
    setEditing(null);
    setPicking(null);
    finder.dismiss();
    setDraft((current) => fromReference(current, valueRegion));
  };

  const saveRef = async (): Promise<void> => {
    if (editing === null || !built.ok || built.spec.kind !== 'characters') {
      return;
    }
    setSaving(true);
    try {
      const saved = await saveReference({
        app: appId,
        referenceName: editing.name.trim(),
        check: built.spec,
        expects: editing.expects.join(''),
      });
      setSaveError(null);
      if (mode.kind === 'reference') {
        await drafting.finished();
        onSaved({ app: saved.app, value: null });
        return;
      }
      await appFile.reload();
      closeEditor();
      setDraft((current) => ({ ...current, kind: 'number', reference: saved.reference }));
    } catch (error: unknown) {
      setSaveError(errorText(error));
    } finally {
      setSaving(false);
    }
  };

  // Why the screenshot shown cannot be removed, or null when it can or the live view shows.
  const valueRegion = editing?.valueRegion ?? null;
  const unremovable =
    selected === null
      ? null
      : finder.view?.screenshot === selected
        ? 'Text found on this screenshot is listed. Add it or select Not these to remove the screenshot.'
        : removeBlocked(draft, valueRegion, selected, looks.view?.recording === true);
  // The screenshot an open search lists text of stays until the list closes.
  const removable = removableRecorded(shots, draft, valueRegion).filter((index) => index !== finder.view?.screenshot);
  // A bar's and a color's fix read with the colors of the first drawing, so they need no more screenshots.
  const offersLooks = draft.kind === 'state' || (fixing === null && draft.kind !== 'number');
  const lookPanel =
    offersLooks || looks.view?.recording === true ? (
      <LookPanel
        view={looks.view}
        listed={listedKept(looks.view?.kept ?? [], shots).length}
        recorded={shots.filter((shot) => shot.recorded).length}
        removable={removable.length}
        blocked={
          otherSize !== null && edited !== null
            ? `Recording needs the window at ${sizeText(edited.screen.windowSize)}, the size ${edited.name} was drawn at.`
            : lookBlocked(draft)
        }
        appName={choice.name}
        error={looks.error}
        onStart={() => {
          background(recordLooks());
        }}
        onStop={() => {
          background(stopLooks());
        }}
        onRemoveAll={() => {
          background(screenshots.remove(removable));
        }}
      />
    ) : null;

  // What the region stays at shows over the picture while the first step sets it, on a picture of the size the
  // region is drawn at.
  const guides =
    step === 'where' &&
    mode.kind === 'value' &&
    editing === null &&
    native !== undefined &&
    drawnSize !== null &&
    sameSize(native, drawnSize) &&
    draft.region !== null &&
    pin !== null
      ? { size: native, region: draft.region, pin }
      : null;
  const named = fixing !== null || valueName.trim().length > 0;
  const ready =
    edit.built === null ? named && hasSide && built.ok && draft.drawnOn !== null : saveable(check.preview, edit.lost);
  const canSave = ready && check.state.kind === 'set' && !busy;
  // A reference that expects more characters than one saves cannot be saved, and the editor says so.
  const overLimit = editing !== null && tooMany(editing.expects);
  const canSaveRef =
    editing !== null && editing.name.trim().length > 0 && built.ok && check.state.kind === 'set' && !busy && !overLimit;
  const whereDone = named && draft.region !== null;
  const fixName = fixing === null ? null : `Fix ${fixing.name}`;
  const flowName = fixName ?? (edited === null ? 'New value' : `Edit ${edited.name}`);
  // The saved reference a fix of a number shares with its first drawing.
  const fixedCheck = fixing?.screen.check;
  const sharedReference = fixedCheck?.kind === 'number' ? fixedCheck.reference : null;
  const editedName = editing === null || editing.name.trim() === '' ? 'New reference' : editing.name.trim();
  // What the reference being edited learns from, and how the characters in it compare with the ones it needs.
  const taught = {
    saved: editing?.existing?.samples.map((sample) => sample.text) ?? [],
    labeled: [...draft.labels.values()].filter((text) => text.trim() !== ''),
    recorded: [...draft.recorded.values()],
    found: draft.found.map((label) => label.text),
  };
  const covered = coverage(
    editing?.expects ?? [],
    knownCharacters([...taught.saved, ...taught.labeled, ...taught.recorded, ...taught.found]),
  );
  // What the recording found, against what the reference needs and knows without it.
  const needs = editing?.expects ?? [];
  const found = discovery(
    needs,
    knownCharacters([...taught.saved, ...taught.labeled, ...taught.found]),
    recording?.readings ?? [],
    review.entries,
    review.checked,
    recording?.shapes.length ?? 0,
  );

  return (
    <main className="capture">
      <section className="stage" aria-label="Capture">
        <div className="bar">
          <span className="crumb">
            {trail.join(' / ')}
            {mode.kind !== 'reference' && editing !== null ? ` / ${flowName}` : ''} /{' '}
            {reviewing ? (
              <>
                {editedName} / <b>Review recording</b>
              </>
            ) : (
              <b>{editing === null ? flowName : editedName}</b>
            )}
          </span>
          {recordingNow !== null ? (
            <span className="pill recording">
              <span className="dot" />
              Recording {clock(recordingNow.elapsedMs)}
            </span>
          ) : reviewing && recording !== null ? (
            <span className="pill">
              {recording.readings.length === 1 ? '1 reading' : `${recording.readings.length} readings`} from{' '}
              {clock(recording.elapsedMs)}
            </span>
          ) : (
            <span className="pill">
              <span className="dot live" />
              Live · {choice.name}
            </span>
          )}
          {native === undefined || reviewing ? null : (
            <span className="pill">
              {native.width} × {native.height}
            </span>
          )}
          {reviewing ? (
            <div className="end">
              <button type="button" className="btn" onClick={review.close}>
                Back
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  background(discard());
                }}
              >
                Discard recording
              </button>
              {settingsChanged ? (
                <button type="button" className="btn" onClick={useSettings}>
                  Use its settings
                </button>
              ) : (
                <button
                  type="button"
                  className="btn"
                  data-record-more
                  disabled={recordingBlocked !== null}
                  onClick={() => {
                    background(record(true));
                  }}
                >
                  Record more
                </button>
              )}
              <button
                type="button"
                className="btn primary"
                disabled={review.keepBlocked !== null}
                aria-describedby={
                  review.keepBlocked === null && review.untaught.readings.length === 0 ? undefined : 'keep-blocked'
                }
                onClick={review.keep}
              >
                {review.keeping.length === 1 ? 'Keep 1 reading' : `Keep ${review.keeping.length} readings`}
              </button>
            </div>
          ) : (
            <div className="end">
              {leavingWindow ? (
                <Question
                  refs={leaveQuestion.refs}
                  label="Change window?"
                  keepLabel="Keep this window"
                  actLabel="Change window"
                  busy={busy}
                  onKeep={() => {
                    setLeavingWindow(false);
                  }}
                  onAct={onChangeWindow}
                >
                  {holds === 'unsaved'
                    ? 'Change window? The screenshots, everything set up on them, and any recording are lost.'
                    : 'Change window? This starts over on the new window. The draft stays as it is unless you save it again.'}
                </Question>
              ) : (
                <button
                  ref={leaveQuestion.control}
                  type="button"
                  className="btn"
                  disabled={busy}
                  onClick={() => {
                    // Finishing a draft starts on its screenshots, which the new window leaves behind.
                    if (holds === 'unsaved' || restored !== null) {
                      setLeavingWindow(true);
                    } else {
                      onChangeWindow();
                    }
                  }}
                >
                  Change window
                </button>
              )}
              <button
                type="button"
                className="btn small"
                aria-pressed={shortcut.choosing}
                onClick={shortcut.toggle}
                onBlur={shortcut.cancel}
                onKeyDown={shortcut.onKeyDown}
              >
                Change shortcut
              </button>
              <button
                type="button"
                className="btn primary"
                onClick={() => {
                  background(screenshots.take());
                }}
              >
                Take screenshot<span className="kbd">{shortcut.label}</span>
              </button>
            </div>
          )}
        </div>
        <p className="note" role="status">
          {shortcut.choosing ? 'Press the new shortcut, or Esc to keep this one.' : null}
        </p>
        {shortcut.error === null ? null : <p className="warn">{shortcut.error}</p>}
        {shortcut.saveError === null ? null : <p className="warn">{shortcut.saveError}</p>}
        {screenshots.error === null ? null : <p className="warn">{screenshots.error}</p>}
        {selection.error === null ? null : <p className="warn">{selection.error}</p>}
        {reviewing && recording !== null ? (
          <ReviewSheet
            view={recording}
            found={found}
            needs={needs.length > 0}
            adds={review.adds}
            entries={review.entries}
            problems={review.flagged}
            crops={recorder.crops}
            clashes={review.clashes}
            untaught={review.untaught}
            blocked={review.keepBlocked}
            error={recordError}
            onEntry={review.edit}
            onKeepAll={review.keepAll}
          />
        ) : null}
        {/* Hidden rather than removed during review, so the view keeps its measured size. */}
        <ZoomView
          picture={layoutSize}
          zoomable={shownPicture !== null}
          hidden={reviewing}
          hint={viewHint(
            recordingNow === null ? null : recordingNow === recording ? 'readings' : 'looks',
            selected,
            shots.length,
            picking !== null,
            liveError,
            shortcut.label,
            selecting,
            area,
            otherSize !== null,
          )}
        >
          {({ space, shown }) => (
            <>
              {reviewing ? null : selected === null ? (
                <LiveCanvas
                  maxWidth={Math.min(LIVE_MAX_WIDTH, Math.max(1, Math.round(space.width * devicePixelRatio)))}
                  onFrame={onFrame}
                  onError={onLiveError}
                />
              ) : shownPicture === null ? null : (
                <PictureCanvas picture={shownPicture} label={`Screenshot ${position(selected)}`} />
              )}
              {shownPicture === null ? null : (
                <MatchOverlay painted={selectedPreview.painted ?? preview.painted} size={shownPicture} />
              )}
              {native === undefined ? null : (
                <RegionSurface
                  size={native}
                  shown={shown}
                  screenshot={selected}
                  region={otherSize === null ? draft.region : null}
                  recording={recordingNow !== null && selected === null}
                  area={area}
                  label={editing !== null ? editedName : regionLabel(valueName, moving)}
                  editable={shownPicture !== null && (otherSize === null || picking !== null)}
                  picking={picking !== null}
                  tool={selecting}
                  ignored={selection.ignored}
                  onRegion={(region) => {
                    setDraft((current) => ({
                      ...current,
                      region,
                      drawnOn: selected,
                      // The reference editor draws for its own characters, which leaves the value's side alone.
                      pin:
                        editing === null
                          ? pinChoiceAfterRegion(current.pin, current.region, region, native)
                          : current.pin,
                    }));
                  }}
                  onPick={pick}
                  onSnap={(rough, combine) => {
                    selection.addStep({ kind: 'snap', region: rough, combine });
                  }}
                  onSelectAt={(point, combine) => {
                    selection.addStep({ kind: 'click', x: point.x, y: point.y, combine });
                  }}
                />
              )}
              {native === undefined ||
              editing === null ||
              finder.view === null ||
              selected !== finder.view.screenshot ? null : (
                <FoundMarks rows={finder.rows} flags={finder.flags} native={native} shown={shown} />
              )}
              {guides === null ? null : <PinGuides shown={shown} {...guides} />}
            </>
          )}
        </ZoomView>
        <div className="strip" hidden={reviewing}>
          <button
            type="button"
            className="thumb"
            aria-pressed={selected === null}
            onClick={() => {
              setSelected(null);
              setPicking(null);
            }}
          >
            <span className="shot">
              <span className="dot live" />
            </span>
            Live
          </button>
          {shots.map((shot) => (
            <button
              key={shot.index}
              type="button"
              className="thumb"
              aria-pressed={selected === shot.index}
              onClick={() => {
                setSelected(shot.index);
              }}
            >
              <span className="shot">
                {shot.thumb === null ? null : <PictureCanvas picture={shot.thumb} label="" />}
              </span>
              <span className="when">
                {shot.recorded ? <span className="dot" role="img" aria-label="Recorded" /> : null}
                {position(shot.index)} · {shot.time}
              </span>
            </button>
          ))}
          {selected === null ? null : (
            <button
              type="button"
              className="btn small"
              aria-disabled={unremovable !== null}
              aria-describedby={unremovable === null ? undefined : 'remove-blocked'}
              onClick={() => {
                // A refused removal keeps the focus, so its reason is heard.
                if (unremovable === null) {
                  background(screenshots.remove([selected]));
                }
              }}
            >
              Remove screenshot {position(selected)}
            </button>
          )}
          <span className="note">
            {unremovable === null ? null : <span id="remove-blocked">{unremovable} </span>}
            Screenshots last until you remove them or change the window.
            {editing === null ? '' : ' A reference learns from each screenshot you label.'}
          </span>
        </div>
      </section>
      {editing === null ? (
        <aside className="inspector" aria-label="Value">
          <div className="stepper" role="group" aria-label="Steps">
            <button
              type="button"
              className="step-tab"
              aria-current={step === 'where' ? 'step' : undefined}
              onClick={() => {
                setStep('where');
              }}
            >
              <span className={whereDone && step !== 'where' ? 'num done' : 'num'}>
                {whereDone && step !== 'where' ? '✓' : '1'}
              </span>
              Where to read
            </button>
            <button
              type="button"
              className="step-tab"
              aria-current={step === 'how' ? 'step' : undefined}
              onClick={() => {
                setStep('how');
              }}
            >
              <span className="num">2</span>
              How to read it
            </button>
          </div>
          {step === 'where' && fixing !== null ? (
            <>
              <div className="group-title">
                <span>
                  {fixing.name} at {drawnSize === null ? "the window's size now" : sizeText(drawnSize)}
                </span>
                <Hint topic="Fix at this size">{FIX_HINT}</Hint>
              </div>
              <p className="note">{fixNote(moving, draft.region !== null)}</p>
              {moving.kind === 'area' ? <p className="note">{moving.limits}</p> : null}
              <RegionFields
                region={draft.region}
                onRegion={(region) => {
                  setDraft((current) => ({ ...current, region, drawnOn: current.drawnOn ?? shownOn }));
                }}
              />
              {lookPanel}
              <div className="sep" />
              {saveError === null ? null : <p className="warn">{saveError}</p>}
              {draftError}
              <div className="actions">
                <button type="button" className="btn" disabled={busy} onClick={onCancel}>
                  Discard
                </button>
                {draftButton}
                <button
                  type="button"
                  className="btn primary"
                  onClick={() => {
                    setStep('how');
                  }}
                >
                  Next: how to read it
                </button>
              </div>
            </>
          ) : step === 'where' ? (
            <>
              {edited === null ? null : <EditIntro value={edited} shown={otherSize} />}
              <div className="field">
                <label htmlFor="value-name">Value name</label>
                <input
                  id="value-name"
                  className="input"
                  placeholder="Progress"
                  value={valueName}
                  onChange={(event) => {
                    setValueName(event.target.value);
                  }}
                />
              </div>
              <div className="field">
                <span className="name">
                  <span id="check-kind">Check</span>
                  <Hint topic="Check">{CHECK_HINT}</Hint>
                </span>
                <div className="seg" role="group" aria-labelledby="check-kind">
                  {CHECK_KINDS.map((kind) => (
                    <button
                      key={kind}
                      type="button"
                      aria-pressed={draft.kind === kind}
                      onClick={() => {
                        setDraft((current) => withKind(current, kind));
                        setPicking(null);
                      }}
                    >
                      {KIND_NAMES[kind]}
                    </button>
                  ))}
                </div>
              </div>
              {draft.kind === 'state' ? (
                <SelectFields
                  tool={tool}
                  colorMatch={draft.colorMatch}
                  canSelect={shownPicture !== null}
                  preview={<SelectedPixelsField preview={selectedPreview} />}
                  onTool={(next) => {
                    setTool(next);
                    setPicking(null);
                  }}
                  onColorMatch={(value) => {
                    setDraft((current) => ({ ...current, colorMatch: value }));
                  }}
                />
              ) : null}
              <RegionFields
                region={draft.region}
                locked={edited !== null && draft.drawnOn === null}
                onRegion={(region) => {
                  // Typed pixels keep to the screenshot the region was drawn on.
                  setDraft((current) => ({ ...current, region, drawnOn: current.drawnOn ?? shownOn }));
                }}
              />
              <MovingContentField moving={moving} appName={choice.name} onMayMove={setMayMove} />
              <PinPicker
                view={staysAt}
                scales={file?.sizeChanges !== 'rarely'}
                onChoice={(pinChoice) => {
                  setDraft((current) => ({ ...current, pin: pinChoice }));
                }}
              />
              {lookPanel}
              <div className="sep" />
              {draftError}
              <div className="actions">
                <button type="button" className="btn" disabled={busy} onClick={onCancel}>
                  Discard
                </button>
                {draftButton}
                <button
                  type="button"
                  className="btn primary"
                  onClick={() => {
                    setStep('how');
                  }}
                >
                  Next: how to read it
                </button>
              </div>
            </>
          ) : (
            <>
              {fixing !== null && draft.kind === 'number' && sharedReference !== null ? (
                <fieldset className="choices" aria-describedby="hint-read-with">
                  <legend className="group-title">
                    <span>Read with</span>
                    <Hint topic="Read with" id="hint-read-with">
                      {READ_WITH_HINT}
                    </Hint>
                  </legend>
                  <label className="choice" aria-label="Shared reference">
                    <input
                      type="radio"
                      name="read-with"
                      aria-describedby="read-with-shared"
                      checked={draft.reference === sharedReference}
                      onChange={() => {
                        setDraft((current) => ({ ...current, reference: sharedReference }));
                      }}
                    />
                    <span>
                      <span className="choice-name">
                        {file?.references.find((reference) => reference.id === sharedReference)?.name ??
                          'Its reference'}
                      </span>
                      <small id="read-with-shared" className="note">
                        The reference it reads with at every size.
                      </small>
                    </span>
                  </label>
                  <label className="choice" aria-label="Learn a reference at this size">
                    <input
                      type="radio"
                      name="read-with"
                      aria-describedby="read-with-own"
                      checked={draft.reference !== sharedReference}
                      onChange={() => {
                        background(
                          openEditor(null, file?.references.find((saved) => saved.id === sharedReference) ?? null),
                        );
                      }}
                    />
                    <span>
                      <span className="choice-name">
                        {draft.reference === sharedReference
                          ? 'Learn a reference at this size'
                          : (file?.references.find((reference) => reference.id === draft.reference)?.name ??
                            'A reference learned at this size')}
                      </span>
                      <small id="read-with-own" className="note">
                        Teach the characters again from this size&apos;s screenshots.
                      </small>
                    </span>
                  </label>
                </fieldset>
              ) : fixing !== null && (draft.kind === 'barFill' || draft.kind === 'color') ? (
                <p className="note">At this size it reads with the colors of its first drawing.</p>
              ) : draft.kind === 'number' ? (
                <ReferencePicker
                  appName={choice.name}
                  file={file}
                  selected={draft.reference}
                  hasRegion={draft.region !== null}
                  whole={draft.whole}
                  onSelect={(reference) => {
                    setDraft((current) => ({ ...current, reference }));
                  }}
                  onEdit={(id) => {
                    background(openEditor(file?.references.find((reference) => reference.id === id) ?? null, null));
                  }}
                  drafted={drafted}
                  onNew={() => {
                    background(openEditor(null, null));
                  }}
                  onRecord={() => {
                    background(recordReadings());
                  }}
                  onWhole={(whole) => {
                    setDraft((current) => ({ ...current, whole }));
                  }}
                />
              ) : edited !== null && keepsStates(draft, edited.screen) ? (
                <SavedStates
                  value={edited}
                  draft={draft}
                  retakeBlocked={
                    selected === null
                      ? 'Select a screenshot that shows the first state.'
                      : otherSize === null
                        ? null
                        : `Select a screenshot at ${sizeText(edited.screen.windowSize)}.`
                  }
                  onDraft={setDraft}
                  onRetake={() => {
                    if (selected !== null) {
                      setDraft((current) => withState(current, selected));
                    }
                  }}
                />
              ) : draft.kind === 'state' ? (
                <StatesField
                  draft={draft}
                  full={full}
                  selected={selected}
                  masks={selection.masks}
                  reading={check.reading}
                  position={position}
                  onDraft={setDraft}
                  onShow={setSelected}
                  fix={fixing !== null}
                />
              ) : (
                <>
                  {draft.kind === 'barFill' ? (
                    <BarDetect bar={bar} picked={draft} position={position} blocked={detectBlocked} />
                  ) : null}
                  <ColorFields
                    draft={draft}
                    picking={picking}
                    canPick={shownPicture !== null}
                    preview={previewField}
                    onDraft={setDraft}
                    onPicking={setPicking}
                  />
                  {draft.kind === 'barFill' ? <BarReadings bar={bar} position={position} /> : null}
                </>
              )}
              {appFile.error === null ? null : <p className="warn">{appFile.error}</p>}
              {lookPanel}
              {check.state.kind === 'set' && check.preview !== null && check.preview.unread !== null ? (
                <p className="reading note">{check.preview.unread}</p>
              ) : (
                <ReadingBox
                  built={trial.ok ? null : trial.next}
                  state={check.state}
                  reading={check.reading}
                  problem={check.problem}
                />
              )}
              {edit.built === null || check.preview === null ? null : (
                <EditOutcome preview={check.preview} agreed={dropFixes.length > 0} onAgree={edit.agree} />
              )}
              <div className="sep" />
              {saveError === null ? null : <p className="warn">{saveError}</p>}
              <p className="note">
                {fixing !== null
                  ? `The fix shows on the Values page, beside ${fixing.name}.`
                  : edited !== null
                    ? `The changes to ${edited.name} show on the Values page, and its automations keep using it.`
                    : savedNote(trail, choice.name)}
              </p>
              {draftError}
              <div className="actions">
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    setStep('where');
                  }}
                >
                  Back
                </button>
                {draftButton}
                <button
                  type="button"
                  className="btn primary"
                  disabled={!canSave}
                  onClick={() => {
                    background(save());
                  }}
                >
                  {fixing !== null ? 'Save fix' : edited === null ? 'Save value' : 'Save changes'}
                </button>
              </div>
            </>
          )}
        </aside>
      ) : (
        <aside className="inspector" aria-label="Reference">
          <ReferenceEditor
            name={editing.name}
            appName={choice.name}
            ink={draft.ink}
            tolerance={draft.tolerance}
            picking={picking === 'ink'}
            canPick={shownPicture !== null}
            selected={selected}
            label={selected === null ? '' : (draft.labels.get(selected) ?? '')}
            saved={taught.saved}
            labeled={taught.labeled}
            recorded={taught.recorded}
            found={taught.found}
            onForgetFound={() => {
              setDraft((current) => ({ ...current, found: [] }));
            }}
            finding={finder.rows.filter((row) => row.keep).map((row) => row.text)}
            expected={editing.expects}
            coverage={covered}
            readers={editing.existing === null || file === null ? [] : readers(file, editing.existing.id)}
            onName={(name) => {
              setEditing((current) => (current === null ? null : { ...current, name }));
            }}
            onPick={() => {
              setPicking(picking === 'ink' ? null : 'ink');
            }}
            onTolerance={(tolerance) => {
              setDraft((current) => ({ ...current, tolerance }));
            }}
            onLabel={(text) => {
              if (selected !== null) {
                setDraft((current) => ({ ...current, labels: new Map(current.labels).set(selected, text) }));
              }
            }}
            onExpected={(expects) => {
              setEditing((current) => (current === null ? null : { ...current, expects }));
            }}
            findPanel={
              selected === null && finder.view === null ? null : (
                <FoundPanel
                  view={finder.view}
                  rows={finder.rows}
                  flags={finder.flags}
                  crops={finder.crops}
                  findBlocked={findBlocked}
                  addBlocked={finder.blocked}
                  error={finder.error}
                  searching={finder.searching}
                  onFind={() => {
                    if (selected !== null) {
                      setPicking(null);
                      background(finder.search(selected));
                    }
                  }}
                  onEntry={finder.edit}
                  onAdd={finder.add}
                  onDismiss={finder.dismiss}
                />
              )
            }
            preview={previewField}
            record={
              <RecordPanel
                view={recording}
                found={found}
                needs={needs.length > 0}
                reviewing={reviewing}
                kept={draft.recorded.size}
                keeping={review.keeping.length}
                blocked={recordingBlocked}
                appName={choice.name}
                crops={recorder.crops}
                error={recordError}
                onStart={() => {
                  background(record(false));
                }}
                onMore={() => {
                  background(record(true));
                }}
                onStop={() => {
                  background(stop());
                }}
                onDiscard={() => {
                  background(discard());
                }}
                onReview={() => {
                  review.show(true);
                }}
                settingsChanged={settingsChanged}
                onUseSettings={useSettings}
              />
            }
          />
          <ReadingBox
            built={built.ok ? null : built.next}
            state={check.state}
            reading={check.reading}
            problem={check.problem}
          />
          <div className="sep" />
          {saveError === null ? null : <p className="warn">{saveError}</p>}
          {overLimit ? <p className="warn">{TOO_MANY}</p> : null}
          {canSaveRef && covered.missing.length > 0 ? (
            <p className="note">
              It has yet to learn every character it needs. You can save now and teach the rest later.
            </p>
          ) : null}
          {draftError}
          <div className="actions">
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => {
                if (mode.kind === 'reference') {
                  onCancel();
                } else {
                  closeEditor();
                }
              }}
            >
              Discard
            </button>
            {draftButton}
            <button
              type="button"
              className="btn primary"
              disabled={!canSaveRef}
              onClick={() => {
                background(saveRef());
              }}
            >
              Save reference
            </button>
          </div>
        </aside>
      )}
    </main>
  );
}
