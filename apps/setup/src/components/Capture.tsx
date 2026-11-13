import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type {
  RecordingScope,
  ReferenceDef,
  ReferenceRef,
  ReferenceTrial,
  TrialSpec,
  ValueDef,
} from '../lib/bindings.ts';
import {
  editValue,
  errorText,
  explainReading,
  listDrafts,
  placeValue,
  saveFix,
  saveReference,
  saveValue,
  setCharacters,
  setCheck,
  tryEdit,
} from '../lib/api.ts';
import {
  type Built,
  type CheckKind,
  type Draft,
  boxBlocked,
  buildCharacters,
  buildSpec,
  EMPTY_DRAFT,
  fixDraft,
  fromReference,
  findBlocked as findBlockedBy,
  onPicture,
  readingOf,
  readsOf,
  charactersOnceTaught,
  recordBlocked,
  recordedPlace,
  regionScreenshot,
  settingsDiffer,
  toReference,
  withFixRegion,
  withRecordedInk,
  withReference,
  withState,
} from '../lib/check.ts';
import { boxNumber, boxOf } from '../lib/boxes.ts';
import { type Placed, useLiveDraw } from '../lib/liveDraw.ts';
import { BoxList, boxTextId } from './BoxList.tsx';
import { DraftButtons } from './capture/DraftButtons.tsx';
import { ReadWith } from './capture/ReadWith.tsx';
import { ScreenshotStage, useScreenshotStage } from './capture/ScreenshotStage.tsx';
import { ChangeWindow, ReviewActions } from './capture/StageBar.tsx';
import { Stepper, nextFromWhere, stepTabs } from './capture/Stepper.tsx';
import { WhereFields } from './capture/WhereFields.tsx';
import { discovery } from '../lib/discovery.ts';
import { agreedFixes, editStart, keepsStates, placedOn, saveable } from '../lib/edit.ts';
import { TOO_MANY, coverage, startingExpected, tooMany } from '../lib/expected.ts';
import { knownCharacters, readers } from '../lib/reference.ts';
import { BarDetect, BarReadings } from './BarFields.tsx';
import { BarColorFields, type ColorTarget, ColorFields } from './CheckFields.tsx';
import { ReadingBox, RegionFields } from './Fields.tsx';
import { ReadingDetailsBox } from './ReadingDetails.tsx';
import { FoundMarks, FoundPanel } from './Found.tsx';
import { listed } from '../lib/found.ts';
import { ReferenceEditor } from './ReferenceEditor.tsx';
import { ReferencePicker } from './ReferencePicker.tsx';
import type { Choice } from './Picker.tsx';
import { colorAt } from '../lib/pixels.ts';
import { RecordPanel, RecordingSpots, ReviewSheet } from './Recording.tsx';
import { CharacterCheckSheet } from './CharacterCheck.tsx';
import { CHECK_OFFER_ID, CheckOffer } from './CheckOffer.tsx';
import { LookPanel } from './LookRecording.tsx';
import { listedKept, lookBlocked, lookTolerance, removableRecorded, removeBlocked } from '../lib/looks.ts';
import { type Point, scaleRect } from '../lib/region.ts';
import { CaptureOutlineNote, CaptureOutlines, useCaptureOutline } from './CaptureOutlines.tsx';
import { RegionSurface } from './RegionSurface.tsx';
import { PinGuides } from './PinGuides.tsx';
import { Hint } from './Hint.tsx';
import { MatchOverlay, MatchPreviewField } from './MatchPreview.tsx';
import { fixNote, isReadingArea, movingContent, regionLabel } from '../lib/moving.ts';
import { type Drawing, stageHint } from '../lib/stageHint.ts';
import { pinChoiceAfterRegion, pinOfView, pinView } from '../lib/pin.ts';
import { FIX_HINT, sameSize, sizeText } from '../lib/sizes.ts';
import type { SelectTool } from '../lib/selection.ts';
import { StatesField } from './StatesField.tsx';
import { EditOutcome, SavedStates } from './ValueEdit.tsx';
import {
  type CaptureKept,
  type CaptureSetup,
  type Editing,
  type Step,
  captureMissing,
  refreshedEditing,
  restoreSetup,
  shownOnStep,
  storeSetup,
} from '../lib/captureDraft.ts';
import { type ReportDraft, type Resumed, draftsOf, isKept, keep, useDraft } from '../lib/drafts.ts';
import { background } from '../lib/repeat.ts';
import { ask } from '../lib/request.ts';
import { type Made, savedNote } from '../lib/flow.ts';
import { draftHolds, referenceSave } from '../lib/teaching.ts';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';
import { useAppFile } from '../lib/useAppFile.ts';
import { useBarSetup } from '../lib/useBarSetup.ts';
import { useBoxes } from '../lib/useBoxes.ts';
import { useFound } from '../lib/useFound.ts';
import { type Trial, trialOf, useLiveCheck } from '../lib/useLiveCheck.ts';
import { useLookRecording } from '../lib/useLookRecording.ts';
import { useMatchPreview, useSelectedPreview } from '../lib/useMatchPreview.ts';
import { useCharacterCheck } from '../lib/useCharacterCheck.ts';
import { progress } from '../lib/confirm.ts';
import { useRecording } from '../lib/useRecording.ts';
import { focusNext, useRecordFocus, useReview } from '../lib/useReview.ts';
import type { Taken } from '../lib/useScreenshots.ts';
import { useStateSelections } from '../lib/useStateSelections.ts';
import { useValueEdit } from '../lib/useValueEdit.ts';

/** The id of the note that says why Save reference waits, which the button points at. */
const SAVE_WAITS_ID = 'save-reference-waits';

/** No characters needed, which filters no find; one list, so nothing that depends on it runs again. */
const NO_NEEDS: readonly string[] = [];

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

/** What came back from the draft the capture finishes when its window was picked. */
export interface Restored {
  /**
   * Which of the draft's screenshots are this session's again, by index, or null once the window changed, which
   * starts the capture again without what the draft kept.
   */
  readonly screenshots: readonly number[] | null;
}

/** What the capture starts with: what the draft `resumed` kept, when `restored` brought it back, or what `mode` makes. */
function startSetup(mode: CaptureMode, resumed: Resumed | null, restored: Restored | null): CaptureSetup {
  const kept = (restored?.screenshots ?? null) === null ? null : (resumed?.def.state ?? null);
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
  /** The draft the flow finishes, or null. */
  readonly resumed: Resumed | null;
  /** What came back from that draft with the window, or null when nothing did. */
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
  resumed,
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
    const setup = startSetup(mode, resumed, restored);
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
  // A draft that kept the reference editor open copied the reference as it was saved then. Once the application's
  // file is read, the editor goes on from the reference as it is saved now, which is also what leaving loses
  // nothing of.
  const savedNow = useRef((restored?.screenshots ?? null) === null);
  useEffect(() => {
    if (file === null || savedNow.current) {
      return;
    }
    savedNow.current = true;
    setEditing((current) => refreshedEditing(current, file.references));
    setStart((began) => ({ ...began, editing: refreshedEditing(began.editing, file.references) }));
  }, [file]);
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
  // Why the reference being taught was not saved. It is the editor's own, and goes when the editor closes, so it
  // never shows as the value's.
  const [referenceError, setReferenceError] = useState<string | null>(null);
  const stage = useScreenshotStage(opening.shots);
  const { screenshots, liveSize, liveError, shownPicture } = stage;
  const { shots, full, selected, setSelected, position } = screenshots;
  const boxes = useBoxes(draft, setDraft, selected);
  const recorder = useRecording();
  const recording = recorder.view;
  // Where the next new recording looks for characters. Record more goes on with the recording's own.
  const [recordScope, setRecordScope] = useState<RecordingScope>('place');
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
  useEffect(
    () =>
      ask(
        listDrafts,
        (drafts) => {
          setDrafted(
            new Set(
              draftsOf(drafts, 'reference').flatMap(({ def }) =>
                def.app === appId && def.changes !== null ? [def.changes] : [],
              ),
            ),
          );
        },
        () => {
          // Without the drafts, nothing is held back.
        },
      ),
    [appId],
  );

  // The saved reference the editor adds to, or null for a new one or while no editor is open.
  const existingId = editing?.existing?.id ?? null;
  const editingRef = useMemo(
    (): ReferenceRef | null => (existingId === null ? null : { app: appId, id: existingId }),
    [existingId, appId],
  );
  const isEditing = editing !== null;
  // The characters the reference editor teaches while it is open, and the check of the value it was opened from.
  const characters = useMemo(
    () => (isEditing ? buildCharacters(draft, editingRef) : null),
    [isEditing, draft, editingRef],
  );
  const valueCheck = useMemo(() => buildSpec(draft, appId), [draft, appId]);
  const built: Built<unknown> = characters ?? valueCheck;
  const builtKind = characters === null ? (valueCheck.ok ? valueCheck.spec.kind : 'none') : 'characters';
  const missing = captureMissing(setup, built, fixing !== null);
  // An edit changes a saved value in place and is never a draft.
  const drafting = useDraft(
    resumed,
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
      <DraftButtons
        resumed={resumed !== null}
        changes={mode.kind === 'fix' || (mode.kind === 'reference' && mode.existing !== null)}
        busy={busy}
        unsaved={holds === 'unsaved'}
        losesRecording={losesRecording}
        onDelete={drafting.discard}
        onSave={() => {
          background(drafting.saveAndClose());
        }}
      />
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
  // The characters it needs, which what it learns from is compared with.
  const needs = editing?.expects ?? NO_NEEDS;
  const recordFocus = useRecordFocus();
  const review = useReview(recording, draft, setDraft, editingRef, needs, recordFocus);
  // What the reference being edited learns from: its saved pictures, its boxes with text, and its kept readings.
  const savedSamples = editing?.existing?.samples;
  const taught = useMemo(
    () => ({
      saved: (savedSamples ?? []).map((sample) => sample.text),
      labeled: draft.boxes.map((box) => box.text).filter((text) => text.trim() !== ''),
      recorded: [...draft.recorded.values()],
    }),
    [savedSamples, draft.boxes, draft.recorded],
  );
  // Every text it learns from already, which tells a find's letters apart from a misread.
  const learnedTexts = useMemo(() => [...taught.saved, ...taught.labeled, ...taught.recorded], [taught]);
  const covered = coverage(needs, knownCharacters(learnedTexts));
  const finder = useFound(draft, boxes.add, editingRef, learnedTexts, needs);
  const { reviewing } = review;
  const guided = useCharacterCheck(finder, review, recorder, needs, {
    boxes: draft.boxes,
    ink: draft.ink,
    tolerance: draft.tolerance,
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
  // The check, like review, shows in place of the window's picture.
  const checking = guided.guide !== null;
  // The recording review shows. Review is open only on a recording, and one name decides everything review changes
  // on the screen, so the bar, the size, and the sheet never disagree.
  const reviewed = reviewing ? recording : null;
  const sheeted = reviewed !== null || checking;
  const checkStep = guided.guide === null ? null : progress(guided.guide, guided.places, needs);
  const recordError = recorder.commandError ?? review.error ?? recorder.pollError;

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
  const outline = useCaptureOutline(appId, native, appFile.file);

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

  // While the value's place is set, it is set on one picture: a new screenshot is that picture, and the region moves
  // onto it when the screenshot has the size the region is drawn at.
  const regionSize = drawnSize ?? edited?.screen.windowSize ?? null;
  const setsPlace = step === 'where' && editing === null;
  const onValuePicture = (shot: Taken | null): void => {
    if (shot === null) {
      return;
    }
    // Until both sizes are known, such as before the screenshot's pixels arrive, the region stays where it is.
    const fits = regionSize === null || shot.size === null ? null : sameSize(regionSize, shot.size);
    setDraft((current) => onPicture(current, shot.index, fits));
  };
  const takeShot = async (): Promise<void> => {
    const shot = await take();
    if (setsPlace) {
      onValuePicture(shot);
    }
  };
  // A drag over the live window takes the screenshot as it starts, and the region drawn goes on it.
  const holdTake = useCallback(async () => take('hold'), [take]);
  const live = useLiveDraw(holdTake, ({ shot, box }: Placed) => {
    if (shot === null) {
      return;
    }
    setSelected(shot.index);
    const size = shot.size;
    const region = box === null || size === null ? null : scaleRect(box.rect, box.at, size);
    if (region === null || size === null) {
      onValuePicture(shot);
      return;
    }
    setDraft((current) => ({
      ...current,
      region,
      drawnOn: shot.index,
      pin: pinChoiceAfterRegion(current.pin, current.region, region, size),
    }));
  });
  const { liveFrozen } = stage;
  useEffect(() => {
    liveFrozen.current = live.isFrozen;
  }, [liveFrozen, live.isFrozen]);
  // The screenshot a press over the live view took shows in place of the frame held still, once its pixels arrive.
  const heldPicture = live.shown.shot === null ? null : (full.get(live.shown.shot.index) ?? null);

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
  const { drawnOn } = draft;
  const fixPin = fixing?.screen.pin;
  // A reference is tried as the number it is made for reads with it once both are saved: with that number's
  // characters, which a fix keeps and a value's follow the reference, whole when it is, and at its side. A reference
  // made on its own reads every character it knows.
  const references = file?.references;
  const taughtTrial = useMemo((): Built<ReferenceTrial> | null => {
    if (characters === null || !characters.ok) {
      return characters;
    }
    const { spec } = characters;
    const reads = readsOf(fixing === null ? charactersOnceTaught(draft, references ?? [], spec) : draft.characters);
    const whole = mode.kind !== 'reference' && draft.whole;
    return { ok: true, spec: { app: appId, characters: spec, reads, whole, pin: fixPin ?? pin } };
  }, [characters, fixing, draft, references, mode.kind, appId, fixPin, pin]);
  const trial = useMemo((): Trial => {
    if (taughtTrial !== null) {
      return trialOf('characters', taughtTrial, { set: setCharacters });
    }
    if (edit.built !== null) {
      return trialOf('edit', edit.built, tryEdit);
    }
    if (!valueCheck.ok) {
      return valueCheck;
    }
    if (drawnOn === null) {
      return { ok: false, next: 'Draw a region on a screenshot.' };
    }
    // A value is tried as it would be saved, which a fix does at its value's side.
    const spec: TrialSpec = { app: appId, check: valueCheck.spec, screenshot: drawnOn, pin: fixPin ?? pin };
    return trialOf('check', { ok: true, spec }, { set: setCheck });
  }, [taughtTrial, edit.built, valueCheck, drawnOn, appId, fixPin, pin]);
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
  // While a reference is taught, the region its pixels are counted in is the selected box on the screenshot shown.
  const chosenOnShown = editing === null ? null : boxes.onShown;
  const previewDraft = editing === null ? draft : { ...draft, region: chosenOnShown?.region ?? null };
  const preview = useMatchPreview(
    editing !== null ? 'ink' : colorFields ? draft.kind : null,
    previewDraft,
    selected,
    shownPicture,
    // An edited value's region is in the pixels of the size it was drawn at, and shows on no other.
    otherSize === null || edited === null
      ? null
      : `Select a screenshot at ${sizeText(edited.screen.windowSize)} to see the matching pixels of its region.`,
  );
  const previewField = <MatchPreviewField preview={preview} region={previewDraft.region} />;
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
    if (!valueCheck.ok || screenshot === null) {
      return null;
    }
    const { spec } = valueCheck;
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
  // The selected box, which Find more like it samples.
  const chosen = boxes.selected;
  const findBlocked = findBlockedBy(draft, chosen);
  // Where the reference being taught reads, and the size of the screenshot that is on, which its boxes keep to.
  const reading = readingOf(draft);
  const readingPicture = reading.drawnOn === null ? undefined : full.get(reading.drawnOn);
  const readingSize =
    readingPicture === undefined ? null : { width: readingPicture.width, height: readingPicture.height };
  const boxBlock = editing === null ? null : boxBlocked(readingSize, shownPicture);

  // Starts a new recording of `from`'s region, or with `more` goes on with the stopped one.
  const record = async (
    more: boolean,
    from: Pick<Draft, 'region' | 'ink' | 'tolerance' | 'drawnOn'> = { ...draft, ...reading },
  ): Promise<void> => {
    const { region, ink, tolerance, drawnOn } = from;
    if (region === null || ink === null || drawnOn === null) {
      return;
    }
    // A reference taught by itself reads at its first box, which the recording is of.
    const readAt = draft.region === null ? (draft.boxes[0]?.id ?? null) : null;
    const scope = more ? (recording?.settings.scope ?? recordScope) : recordScope;
    // Each box is one more example of the text a recording of the whole window looks for.
    const examples = draft.boxes.map((box) => ({ region: box.region, screenshot: box.screenshot }));
    const settings = { app: appId, pin: fixPin ?? pin, region, ink, tolerance, drawnOn, scope, examples };
    await recorder.start(settings, more, () => {
      review.show(false);
      setSelected(null);
      setPicking(null);
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

  // The stopped recording's settings, when the draft no longer has them, so
  // Record more can offer to take them back.
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
    setDraft((current) => withRecordedInk(current, settings));
    recorder.clearCommandError();
    // The pressed button gives way to Record more, so focus moves to it.
    focusNext(recordFocus.more);
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
    setEditing({ existing, name: existing?.name ?? '', expects: startingExpected(existing, like) });
    setDraft((current) => toReference(current, existing));
    boxes.select(null);
    setPicking(null);
    setSaveError(null);
    setReferenceError(null);
    await looks.discard();
  };

  // Opens the reference editor on the picked reference, or on a new one, to
  // record the value's region. A saved reference has its characters' color,
  // so recording starts at once; a new one has none, so picking it is next.
  const recordReadings = async (): Promise<void> => {
    const existing = file?.references.find((reference) => reference.id === draft.reference) ?? null;
    if (existing !== null && draftHolds(existing, drafted) !== null) {
      return;
    }
    const { region, drawnOn } = draft;
    await openEditor(existing, null);
    if (region === null || drawnOn === null) {
      return;
    }
    if (existing === null) {
      setSelected(drawnOn);
      setPicking('ink');
      focusNext(recordFocus.title);
      return;
    }
    await record(false, { region, drawnOn, ink: existing.ink, tolerance: existing.tolerance });
  };

  // Leaves the reference editor for the value, forgetting its boxes and recording.
  const closeEditor = (): void => {
    if (recording !== null) {
      background(discard());
    }
    setEditing(null);
    setReferenceError(null);
    setPicking(null);
    boxes.select(null);
    finder.dismiss();
    guided.forget();
    setDraft(fromReference);
  };

  const saveRef = async (): Promise<void> => {
    if (editing === null || characters === null || !characters.ok) {
      return;
    }
    setSaving(true);
    try {
      const saved = await saveReference({
        app: appId,
        referenceName: editing.name.trim(),
        characters: characters.spec,
        expects: editing.expects.join(''),
      });
      setReferenceError(null);
      if (mode.kind === 'reference') {
        await drafting.finished();
        onSaved({ app: saved.app, value: null });
        return;
      }
      // The choice of characters was made against the references as they were before this save.
      const before = references ?? [];
      const after = (await appFile.reload())?.references ?? [];
      closeEditor();
      // A value's choice of characters follows its reference, as when one is picked. A fix reads its value's.
      setDraft((current) =>
        fixing === null
          ? withReference({ ...current, kind: 'number' }, saved.reference, after, before)
          : { ...current, kind: 'number', reference: saved.reference },
      );
    } catch (error: unknown) {
      setReferenceError(errorText(error));
    } finally {
      setSaving(false);
    }
  };

  // Why the screenshot shown cannot be removed, or null when it can or the live view shows.
  const unremovable =
    selected === null
      ? null
      : finder.view?.screenshot === selected
        ? 'Text found on this screenshot is listed. Add it or select Not these to remove the screenshot.'
        : removeBlocked(draft, selected, looks.view?.recording === true);
  // The screenshot an open search lists text of stays until the list closes.
  const removable = removableRecorded(shots, draft).filter((index) => index !== finder.view?.screenshot);
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
  const refSave = referenceSave({
    named: editing !== null && editing.name.trim().length > 0,
    built: built.ok,
    tried: check.state.kind === 'set',
    busy,
    overLimit,
    reviewing: reviewed !== null,
  });
  const whereDone = named && draft.region !== null;
  const fixName = fixing === null ? null : `Fix ${fixing.name}`;
  const flowName = fixName ?? (edited === null ? 'New value' : `Edit ${edited.name}`);
  // The saved reference a fix of a number shares with its first drawing.
  const fixedCheck = fixing?.screen.check;
  const sharedReference = fixedCheck?.kind === 'number' ? fixedCheck.reference : null;
  const editedName = editing === null || editing.name.trim() === '' ? 'New reference' : editing.name.trim();
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

  // A number's characters and its check are steps of their own; other kinds read and save in their second step.
  const threeSteps = draft.kind === 'number' && fixing === null;
  const shownStep: Step = !threeSteps && step === 'check' ? 'how' : step;
  // The value's place is drawn over the live window while it has none, and the press takes the screenshot.
  const drawsLive =
    shownStep === 'where' &&
    editing === null &&
    selected === null &&
    draft.region === null &&
    liveSize !== null &&
    recordingNow === null &&
    !sheeted &&
    picking === null;
  const drawing: Drawing = drawsLive ? 'live' : editing !== null ? 'boxes' : 'region';
  // While the value's place is set, the strip holds the one screenshot it is on, or the one shown.
  const placeShot = draft.drawnOn ?? selected;
  const stripShots = setsPlace ? shots.filter((shot) => shot.index === placeShot) : shots;
  // Where to read lists one screenshot, so entering it shows that one.
  const goTo = (next: Step): void => {
    setStep(next);
    setSelected(shownOnStep(next, draft.drawnOn, selected));
  };
  // While a reference is taught, the steps show where the value is, and wait until the reference is saved or discarded.
  const stepper = (
    <Stepper
      tabs={stepTabs(threeSteps)}
      current={editing === null ? shownStep : 'how'}
      done={(tab) => (tab === 'where' ? whereDone : tab === 'how' ? threeSteps && draft.reference !== null : false)}
      locked={editing !== null}
      onStep={goTo}
    />
  );
  // A number reads with a reference, which its second step picks, teaches, or records readings for.
  const referencePicker = (
    <ReferencePicker
      appName={choice.name}
      file={file}
      selected={draft.reference}
      hasRegion={draft.region !== null}
      whole={draft.whole}
      characters={draft.characters}
      onSelect={(reference) => {
        setDraft((current) => withReference(current, reference, file?.references ?? []));
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
      onCharacters={(characters) => {
        setDraft((current) => ({ ...current, characters }));
      }}
    />
  );
  const nextFromFirst = (
    <button
      type="button"
      className="btn primary"
      onClick={() => {
        goTo('how');
      }}
    >
      {nextFromWhere(threeSteps)}
    </button>
  );

  const colorsOf = {
    draft,
    picking,
    canPick: shownPicture !== null,
    preview: previewField,
    onPicking: setPicking,
  };
  const firstDrawing = <p className="note">At this size it reads with the colors of its first drawing.</p>;
  // What each kind of check is set up with in the step that reads and saves it, keyed by kind, so a new kind does
  // not compile until it has fields. A number's reference has a step of its own, so here it only has a fix's.
  const howFields: Readonly<Record<CheckKind, () => ReactNode>> = {
    number: () =>
      fixing === null || sharedReference === null ? null : (
        <ReadWith
          references={file?.references ?? null}
          shared={sharedReference}
          selected={draft.reference}
          onShared={() => {
            setDraft((current) => ({ ...current, reference: sharedReference }));
          }}
          onLearn={(like) => {
            background(openEditor(null, like));
          }}
        />
      ),
    barFill: () =>
      fixing === null ? (
        <>
          <BarDetect bar={bar} picked={draft} position={position} blocked={detectBlocked} />
          <BarColorFields {...colorsOf} />
          <BarReadings bar={bar} position={position} />
        </>
      ) : (
        firstDrawing
      ),
    color: () => (fixing === null ? <ColorFields {...colorsOf} onDraft={setDraft} /> : firstDrawing),
    state: () =>
      edited !== null && keepsStates(draft, edited.screen) ? (
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
      ) : (
        <StatesField
          draft={draft}
          full={full}
          selected={selected}
          masks={selection.masks}
          reads={check.reads}
          position={position}
          onDraft={setDraft}
          onShow={setSelected}
          fix={fixing !== null}
        />
      ),
  };

  // What shows in place of the window's picture, with its buttons: the check of the characters, or review.
  const sheet =
    guided.guide !== null
      ? {
          actions: (
            <button type="button" className="btn" onClick={guided.close}>
              {reviewing ? 'Back to review' : 'Back to the screenshot'}
            </button>
          ),
          content: (
            <CharacterCheckSheet
              guide={guided.guide}
              places={guided.places}
              needs={needs}
              picture={guided.picture}
              onTurn={guided.act}
              onEdit={guided.edit}
              onClose={guided.close}
            />
          ),
        }
      : reviewed !== null
        ? {
            actions: (
              <ReviewActions
                review={review}
                settingsChanged={settingsChanged}
                recordBlocked={recordingBlocked}
                recordMore={recordFocus.more}
                onDiscard={() => {
                  background(discard());
                }}
                onUseSettings={useSettings}
                onRecordMore={() => {
                  background(record(true));
                }}
              />
            ),
            content: (
              <ReviewSheet
                view={reviewed}
                found={found}
                needs={needs}
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
                onCheck={guided.places.length === 0 ? null : guided.open}
              />
            ),
          }
        : null;

  return (
    <main className="capture">
      <ScreenshotStage
        stage={stage}
        label="Capture"
        bar={{
          path: [
            ...trail,
            ...(mode.kind !== 'reference' && editing !== null ? [flowName] : []),
            ...(sheeted ? [editedName] : []),
            ...(checking && reviewed !== null ? ['Review recording'] : []),
          ],
          current: checking
            ? 'Check characters'
            : reviewed !== null
              ? 'Review recording'
              : editing === null
                ? flowName
                : editedName,
          status:
            recordingNow !== null
              ? { kind: 'recording', elapsedMs: recordingNow.elapsedMs }
              : checking
                ? { kind: 'checking', step: checkStep }
                : reviewed !== null
                  ? { kind: 'reviewing', recording: reviewed }
                  : { kind: 'live', appName: choice.name },
          size: sheeted ? null : (native ?? null),
        }}
        actions={
          <ChangeWindow
            busy={busy}
            // Finishing a draft starts on its screenshots, which the new window leaves behind.
            loses={holds === 'unsaved' ? 'unsaved' : resumed === null ? 'nothing' : 'draft'}
            onChangeWindow={onChangeWindow}
          />
        }
        onTake={takeShot}
        sheet={sheet}
        notes={selection.error === null ? null : <p className="warn">{selection.error}</p>}
        held={{ frozen: live.shown.frozen, picture: heldPicture }}
        hint={(shortcut) =>
          stageHint({
            recording: recordingNow === null ? null : recordingNow === recording ? 'readings' : 'looks',
            selected,
            shotCount: shots.length,
            picking: picking !== null,
            liveError,
            shortcut,
            tool: selecting,
            area,
            otherSize: otherSize !== null,
            drawing,
            boxBlock,
          })
        }
        strip={{
          shots: stripShots,
          removal: {
            unremovable,
            onRemove: (index) => {
              background(screenshots.remove([index]));
            },
          },
          note: `Screenshots last until you remove them or change the window.${
            setsPlace ? ' The value needs one screenshot; take another to draw on a newer one.' : ''
          }`,
          onSelect: (index) => {
            if (index === null) {
              setPicking(null);
            }
          },
        }}
        foot={outline === null || sheeted ? null : <CaptureOutlineNote outline={outline} />}
      >
        {({ shown }) => (
          <>
            {shownPicture === null ? null : (
              <MatchOverlay painted={selectedPreview.painted ?? preview.painted} size={shownPicture} />
            )}
            {native === undefined || outline === null ? null : (
              <CaptureOutlines outline={outline} size={native} shown={shown} />
            )}
            {recording?.recording === true && selected === null ? (
              <RecordingSpots view={recording} shown={shown} />
            ) : null}
            {drawsLive ? (
              <RegionSurface
                size={liveSize}
                shown={shown}
                screenshot={null}
                // The box drawn stays outlined from when the pointer is let go until its screenshot arrives.
                region={live.shown.box?.rect ?? null}
                area={area}
                label={regionLabel(valueName, moving)}
                editable={live.shown.box === null}
                onPointer={(phase) => {
                  if (phase === 'down') {
                    live.press();
                  } else {
                    live.release();
                  }
                }}
                onRegion={(rect) => {
                  live.drew({ rect, at: liveSize });
                }}
              />
            ) : editing !== null ? (
              shownPicture === null ? null : (
                <RegionSurface
                  size={shownPicture}
                  shown={shown}
                  screenshot={selected}
                  region={chosenOnShown?.region ?? null}
                  label={chosenOnShown === null ? '' : `Box ${boxNumber(draft.boxes, chosenOnShown.id)}`}
                  editable={boxBlock === null || picking !== null}
                  others={boxes.others}
                  onSelectOther={boxes.select}
                  describedBy="hint-boxes"
                  onDelete={() => {
                    if (chosenOnShown !== null) {
                      boxes.remove(chosenOnShown.id);
                    }
                  }}
                  onRegion={(region, made) => {
                    const drawn = boxes.draw(region, made);
                    if (drawn !== null) {
                      // What the new box shows is typed next, so focus goes to its field in the list.
                      requestAnimationFrame(() => {
                        document.getElementById(boxTextId(drawn))?.focus();
                      });
                    }
                  }}
                  pick={picking === null ? null : pick}
                />
              )
            ) : native === undefined ? null : (
              <RegionSurface
                size={native}
                shown={shown}
                screenshot={selected}
                region={otherSize === null ? draft.region : null}
                recording={recordingNow !== null && selected === null}
                area={area}
                label={regionLabel(valueName, moving)}
                editable={shownPicture !== null && (otherSize === null || picking !== null)}
                tool={selecting}
                ignored={selection.ignored}
                onRegion={(region) => {
                  setDraft((current) => ({
                    ...current,
                    region,
                    drawnOn: selected,
                    pin: pinChoiceAfterRegion(current.pin, current.region, region, native),
                  }));
                }}
                pick={picking === null ? null : pick}
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
              <FoundMarks rows={listed(finder.rows)} flags={finder.flags} native={native} shown={shown} />
            )}
            {guides === null ? null : <PinGuides shown={shown} {...guides} />}
          </>
        )}
      </ScreenshotStage>
      {editing === null ? (
        <aside className="inspector" aria-label="Value">
          {stepper}
          {shownStep === 'where' && fixing !== null ? (
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
              <div className="sep" />
              {saveError === null ? null : <p className="warn">{saveError}</p>}
              {draftError}
              <div className="actions">
                <button type="button" className="btn" disabled={busy} onClick={onCancel}>
                  Discard
                </button>
                {draftButton}
                {nextFromFirst}
              </div>
            </>
          ) : shownStep === 'where' ? (
            <>
              <WhereFields
                edited={edited}
                otherSize={otherSize}
                valueName={valueName}
                draft={draft}
                tool={tool}
                canSelect={shownPicture !== null}
                selectedPreview={selectedPreview}
                shownOn={shownOn}
                moving={moving}
                appName={choice.name}
                staysAt={staysAt}
                scales={file?.sizeChanges !== 'rarely'}
                onValueName={setValueName}
                onDraft={setDraft}
                onTool={setTool}
                onMayMove={setMayMove}
                onStopPicking={() => {
                  setPicking(null);
                }}
              />
              <div className="sep" />
              {draftError}
              <div className="actions">
                <button type="button" className="btn" disabled={busy} onClick={onCancel}>
                  Discard
                </button>
                {draftButton}
                {nextFromFirst}
              </div>
            </>
          ) : shownStep === 'how' && threeSteps ? (
            <>
              {referencePicker}
              {appFile.error === null ? null : <p className="warn">{appFile.error}</p>}
              <div className="sep" />
              {draftError}
              <div className="actions">
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    goTo('where');
                  }}
                >
                  Back
                </button>
                {draftButton}
                <button
                  type="button"
                  className="btn primary"
                  onClick={() => {
                    goTo('check');
                  }}
                >
                  Next: check and save
                </button>
              </div>
            </>
          ) : (
            <>
              {howFields[draft.kind]()}
              {appFile.error === null ? null : <p className="warn">{appFile.error}</p>}
              {lookPanel}
              {check.state.kind === 'set' && check.preview !== null && check.preview.unread !== null ? (
                <p className="reading note">{check.preview.unread}</p>
              ) : (
                <ReadingBox built={trial.ok ? null : trial.next} state={check.state} reads={check.reads} />
              )}
              {edit.built === null || check.preview === null ? null : (
                <EditOutcome preview={check.preview} agreed={dropFixes.length > 0} onAgree={edit.agree} />
              )}
              {/* Keyed by the check's kind, so scores of the last kind never show under a new one. */}
              <ReadingDetailsBox key={builtKind} ask={check.state.kind === 'set' ? explainReading : null} />
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
                    goTo(threeSteps ? 'how' : 'where');
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
          {mode.kind === 'reference' ? null : stepper}
          <ReferenceEditor
            name={editing.name}
            appName={choice.name}
            ink={draft.ink}
            tolerance={draft.tolerance}
            picking={picking === 'ink'}
            canPick={shownPicture !== null}
            saved={taught.saved}
            labeled={taught.labeled}
            recorded={taught.recorded}
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
            onExpected={(expects) => {
              setEditing((current) => (current === null ? null : { ...current, expects }));
            }}
            boxes={
              <BoxList
                boxes={draft.boxes}
                selected={boxes.selectedId}
                full={full}
                position={position}
                blocked={boxBlock}
                // A reference taught from a value reads at the value's region, not at a box.
                readsAt={draft.region === null ? (draft.boxes[0]?.id ?? null) : null}
                onSelect={(id) => {
                  const box = boxOf(draft.boxes, id);
                  boxes.select(id);
                  if (box !== null) {
                    setSelected(box.screenshot);
                  }
                }}
                onText={boxes.retype}
                onDelete={boxes.remove}
              />
            }
            findPanel={
              chosen === null && finder.view === null ? null : (
                <FoundPanel
                  finder={finder}
                  findBlocked={findBlocked}
                  onCheck={checking ? null : guided.open}
                  onFind={() => {
                    if (chosen !== null) {
                      setPicking(null);
                      setSelected(chosen.screenshot);
                      background(finder.search(chosen));
                    }
                  }}
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
                scope={recordScope}
                onScope={setRecordScope}
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
                focus={recordFocus}
              />
            }
          />
          <ReadingBox built={built.ok ? null : built.next} state={check.state} reads={check.reads} />
          {/* Keyed by the check's kind, so scores of the last kind never show under a new one. */}
          <ReadingDetailsBox key={builtKind} ask={check.state.kind === 'set' ? explainReading : null} />
          <div className="sep" />
          {referenceError === null ? null : <p className="warn">{referenceError}</p>}
          {overLimit ? <p className="warn">{TOO_MANY}</p> : null}
          {refSave.waits === null ? null : (
            <p id={SAVE_WAITS_ID} className="note">
              {refSave.waits}
            </p>
          )}
          {refSave.can && covered.missing.length > 0 ? (
            <p className="note">
              It has yet to learn every character it needs. You can save now and teach the rest later.
            </p>
          ) : null}
          {draftError}
          {checking ? null : <CheckOffer characters={guided.unconfirmed} onCheck={guided.open} />}
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
              disabled={!refSave.can}
              aria-describedby={
                refSave.waits !== null
                  ? SAVE_WAITS_ID
                  : checking || guided.unconfirmed.length === 0
                    ? undefined
                    : CHECK_OFFER_ID
              }
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
