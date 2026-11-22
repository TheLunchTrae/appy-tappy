import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { DraftShot, ReferenceDef, ReferenceRef, ReferenceTrial, TrialSpec, ValueDef } from '../lib/bindings.ts';
import {
  editValue,
  errorText,
  explainReading,
  placeValue,
  saveFix,
  saveValue,
  setCharacters,
  setCheck,
  tryEdit,
} from '../lib/api.ts';
import {
  type Built,
  type CheckKind,
  type Draft,
  type ReferenceSaving,
  buildCharacters,
  buildReferenceSave,
  buildSpec,
  savedNeeds,
  EMPTY_DRAFT,
  firstWithPixels,
  fixDraft,
  onPicture,
  regionScreenshot,
  withFixRegion,
  withState,
} from '../lib/check.ts';
import { boxNumber } from '../lib/boxes.ts';
import { type Placed, liveDrawFrame, regionFromLiveBox, regionOverFrame, useLiveDraw } from '../lib/liveDraw.ts';
import { boxTextId } from './BoxList.tsx';
import { DraftButtons } from './capture/DraftButtons.tsx';
import { ReadWith } from './capture/ReadWith.tsx';
import { useLanding } from '../lib/landing.ts';
import { showsNewCloseSign } from '../lib/windowSearch.ts';
import { ScreenshotStage, useScreenshotStage } from './capture/ScreenshotStage.tsx';
import { ChangeWindow } from './capture/StageBar.tsx';
import { Stepper, nextFromWhere, stepTabs } from './capture/Stepper.tsx';
import { WhereFields } from './capture/WhereFields.tsx';
import { agreedFixes, editStart, keepsStates, placedOn, saveable } from '../lib/edit.ts';
import { BarDetect, BarReadings } from './BarFields.tsx';
import { BarColorFields, type ColorTarget, ColorFields } from './CheckFields.tsx';
import { ReadingBox, RegionFields } from './Fields.tsx';
import { ReadingDetailsBox } from './ReadingDetails.tsx';
import { FoundMarks } from './Found.tsx';
import { listed } from '../lib/found.ts';
import { ReferenceInspector, teachingSheet } from './ReferenceInspector.tsx';
import { ReferencePicker } from './ReferencePicker.tsx';
import type { Choice } from './Picker.tsx';
import { colorAt } from '../lib/pixels.ts';
import { RecordingSpots } from './Recording.tsx';
import { LookPanel } from './LookRecording.tsx';
import { listedKept, lookBlocked, lookTolerance, removableRecorded, removeBlocked } from '../lib/looks.ts';
import type { Point } from '../lib/region.ts';
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
  keptForReference,
  referenceForValue,
  refreshedEditing,
  restoreSetup,
  regionOnStage,
  shownOnStep,
  storeSetup,
  startEditing,
} from '../lib/captureDraft.ts';
import { type ReportDraft, type Resumed, isKept, keep, saveBeside, useDraft } from '../lib/drafts.ts';
import { background } from '../lib/repeat.ts';
import { type Made, savedNote } from '../lib/flow.ts';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';
import { useAppFile } from '../lib/useAppFile.ts';
import { useBarSetup } from '../lib/useBarSetup.ts';
import { useBoxes } from '../lib/useBoxes.ts';
import { type Trial, trialOf, useLiveCheck } from '../lib/useLiveCheck.ts';
import { useLookRecording } from '../lib/useLookRecording.ts';
import { useReferenceTeaching } from '../lib/useReferenceTeaching.ts';
import { useMatchPreview, useSelectedPreview } from '../lib/useMatchPreview.ts';
import type { Taken } from '../lib/useScreenshots.ts';
import { useStateSelections } from '../lib/useStateSelections.ts';
import { useValueEdit } from '../lib/useValueEdit.ts';

/**
 * What the capture screen makes: a value; a reference, new or saved, on the References page; a fix, a saved value
 * drawn again at the window's size now; or an edit, a saved value changed as it was first drawn.
 */
export type CaptureMode =
  | { readonly kind: 'value' }
  | { readonly kind: 'reference'; readonly existing: ReferenceDef | null }
  | { readonly kind: 'fix'; readonly value: ValueDef }
  | { readonly kind: 'edit'; readonly value: ValueDef };

/** What the reference editor's save waits for while no reference is edited, which never shows. */
const NO_REFERENCE_SAVE: Built<ReferenceSaving> = { ok: false, next: '' };

/** The characters outside a reference's capture, which reads none. */
const NO_CHARACTERS: readonly string[] = [];

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
   * Which of the draft's screenshots are this session's again, each with whether a recording of looks kept it, or null
   * once the window changed, which starts the capture again without what the draft kept.
   */
  readonly screenshots: readonly DraftShot[] | null;
}

/** What the capture starts with: what the draft `resumed` kept, when `restored` brought it back, or what `mode` makes. */
function startSetup(mode: CaptureMode, resumed: Resumed | null, restored: Restored | null): CaptureSetup {
  const kept = (restored?.screenshots ?? null) === null ? null : (resumed?.def.state ?? null);
  if (kept !== null && mode.kind !== 'edit' && isKept<CaptureKept>(kept, mode.kind)) {
    return restoreSetup(kept.state, mode.kind === 'reference' ? mode.existing : undefined);
  }
  const editing: Editing | null = mode.kind === 'reference' ? startEditing(mode.existing) : null;
  if (mode.kind === 'edit') {
    return { step: 'where', editing, ...editStart(mode.value) };
  }
  return {
    valueName: '',
    step: 'where',
    mayMove: false,
    editing,
    draft: mode.kind === 'fix' ? fixDraft(mode.value.screen.check) : EMPTY_DRAFT,
  };
}

interface Props {
  /** The saved application the value or reference belongs to. */
  readonly choice: Pick<Choice, 'app' | 'name'>;
  /**
   * What shows in place of the window while it is not captured, and why what needs the window waits, or null once it
   * is captured. Everything that needs no window goes on without it.
   */
  readonly absent: { readonly panel: ReactNode; readonly reason: string } | null;
  /** What the flow says about the window, above the picture. */
  readonly windowNotes: ReactNode;
  /**
   * Hears each sign that the window may have closed: a failure of its live view, a refused screenshot, and a
   * recording that stopped or could not start.
   */
  readonly onWindowFailed: () => void;
  /**
   * Whether the flow shows because another flow ended, so the focus has to land somewhere, and the control it lands
   * on, or null while that is not known.
   */
  readonly landing: { readonly owed: boolean; readonly target: string | null };
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
  /**
   * Closes a new value's flow once **Save and make a reference** saved it as a draft, and opens the draft of a
   * reference it saved beside it; null where the button is not offered.
   */
  readonly onMakeReference: ((reference: Resumed) => void) | null;
  /** Hears what leaving the capture would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the capture's draft. */
  readonly onDraft: ReportDraft;
}

export function Capture({
  choice,
  absent,
  windowNotes,
  onWindowFailed,
  landing,
  mode,
  resumed,
  restored,
  trail,
  onCancel,
  onChangeWindow,
  onSaved,
  onDrafted,
  onMakeReference,
  onHeld,
  onDraft,
}: Props) {
  // What the capture opens with: what its draft kept, or else what its mode makes.
  const [opening] = useState(() => ({
    setup: startSetup(mode, resumed, restored),
    shots: restored?.screenshots ?? [],
  }));
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
  const appId = choice.app;
  const windowless = absent?.reason ?? null;
  const page = useRef<HTMLElement>(null);
  useLanding(landing.owed, landing.target, page);
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
    setStart((began) => ({
      ...began,
      editing: refreshedEditing(began.editing, file.references),
    }));
  }, [file]);
  const [picking, setPicking] = useState<ColorTarget | null>(null);
  const [tool, setTool] = useState<SelectTool>('box');
  const [valueName, setValueName] = useState(start.valueName);
  const fixing = mode.kind === 'fix' ? mode.value : null;
  // The saved value an edit changes as it was first drawn.
  const edited = mode.kind === 'edit' ? mode.value : null;
  // The characters the saved value needs already, which it keeps however many they are.
  const needsSaved = useMemo(
    () => (mode.kind === 'fix' || mode.kind === 'edit' ? savedNeeds(mode.value.screen.check) : null),
    [mode],
  );
  // Whether the user said the value's content may move around inside the window.
  const [mayMove, setMayMove] = useState(start.mayMove);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const stage = useScreenshotStage(opening.shots);
  const { screenshots, liveSize, liveError, shownPicture } = stage;
  const { shots, full, selected, setSelected, position } = screenshots;
  const boxes = useBoxes(draft, setDraft, selected);
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
  const setup: CaptureSetup = {
    valueName,
    step,
    mayMove,
    editing,
    draft,
  };
  // The saved reference a reference's capture adds to, or null for a new one or outside a reference's capture.
  const existingId = editing?.existing?.id ?? null;
  const editingRef = useMemo(
    (): ReferenceRef | null => (existingId === null ? null : { app: appId, id: existingId }),
    [existingId, appId],
  );
  const isEditing = editing !== null;
  // What the reference reads, which it is tried and saved with.
  const reads = editing?.reads ?? NO_CHARACTERS;
  // The characters a reference's capture teaches, and the check of every other capture.
  const characters = useMemo(
    () => (isEditing ? buildCharacters(draft, editingRef, reads) : null),
    [isEditing, draft, editingRef, reads],
  );
  const valueCheck = useMemo(() => buildSpec(draft, appId, needsSaved), [draft, appId, needsSaved]);
  const referenceSave = useMemo(
    () => (isEditing ? buildReferenceSave(draft, editingRef, reads) : null),
    [isEditing, draft, editingRef, reads],
  );
  const built: Built<unknown> = referenceSave ?? valueCheck;
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
  const selection = useStateSelections(draft, setDraft, selected);
  const selecting = draft.kind === 'state' ? tool : 'box';
  const bar = useBarSetup(
    draft,
    setDraft,
    shots.map((each) => each.index),
    step === 'how' && editing === null && fixing === null,
  );
  // The window size the region's pixels refer to.
  const drawnPicture = draft.drawnOn === null ? undefined : full.get(draft.drawnOn);
  const drawnSize = drawnPicture === undefined ? null : { width: drawnPicture.width, height: drawnPicture.height };
  // An edit's region keeps to the size the value was drawn at while no screenshot of that size holds it.
  const staysAt = pinView(draft.pin, draft.region, drawnSize ?? edited?.screen.windowSize ?? null);
  const pin = pinOfView(staysAt);
  const fixPin = fixing?.screen.pin;
  const teaching = useReferenceTeaching({
    app: appId,
    editing,
    draft,
    setDraft,
    boxes,
    reference: editingRef,
    saving: referenceSave,
    windowless,
    full,
    shown: shownPicture,
    setSelected,
    stopPicking: () => {
      setPicking(null);
    },
    onSaving: setSaving,
    onSaved: async (saved) => {
      await drafting.finished();
      onSaved({ app: saved.app, value: null });
    },
  });
  const { recording, finder, reviewed, sheeted, checking } = teaching;
  // The recording that runs now, of readings or of looks: the owner records one thing at a time.
  const recordingNow = recording?.recording === true ? recording : looks.view?.recording === true ? looks.view : null;
  const closeSigns = useMemo(
    () => [screenshots.error, recording?.problem ?? null, looks.view?.problem ?? null, looks.error],
    [screenshots.error, recording?.problem, looks.view?.problem, looks.error],
  );
  const closeSignsShown = useRef(closeSigns);
  useEffect(() => {
    if (showsNewCloseSign(closeSignsShown.current, closeSigns)) {
      onWindowFailed();
    }
    closeSignsShown.current = closeSigns;
  }, [closeSigns, onWindowFailed]);
  // A window that goes shows its panel, whatever screenshot showed, so the way back to it is in view.
  const hasWindow = windowless === null;
  useEffect(() => {
    if (!hasWindow) {
      setSelected(null);
    }
  }, [hasWindow, setSelected]);
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
  const first = firstWithPixels(shots, full);
  const firstShot = first?.index ?? null;
  const firstPicture = first?.picture;
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
    // Until the region's size is known, the region stays where it is.
    const fits = regionSize === null ? null : sameSize(regionSize, shot.size);
    setDraft((current) => onPicture(current, shot.index, fits, edited === null ? 'draw' : 'edit'));
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
    const size = shot.size;
    const fits = regionSize !== null && sameSize(regionSize, size);
    const region = regionFromLiveBox(box, size, edited?.screen.windowSize ?? null);
    if (region === null) {
      // A press that places nothing takes a screenshot, but leaves a region placed at another size where it is.
      if (draft.region === null || fits) {
        setSelected(shot.index);
        onValuePicture(shot);
      } else {
        background(screenshots.remove([shot.index]));
      }
      return;
    }
    setSelected(shot.index);
    setDraft((current) => ({
      ...current,
      region,
      drawnOn: shot.index,
      pin: pinChoiceAfterRegion(current.pin, fits ? current.region : null, region, size),
    }));
  });
  const { liveFrozen } = stage;
  useEffect(() => {
    liveFrozen.current = live.isFrozen;
  }, [liveFrozen, live.isFrozen]);
  // The screenshot a press over the live view took shows in place of the frame held still, once its pixels arrive.
  const heldPicture = live.shown.shot === null ? null : (full.get(live.shown.shot.index) ?? null);

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
  // A reference is tried as it reads alone: every character it knows, with the digits after a decimal point.
  const taughtTrial = useMemo((): Built<ReferenceTrial> | null => {
    if (characters === null || !characters.ok) {
      return characters;
    }
    return { ok: true, spec: { app: appId, characters: characters.spec } };
  }, [characters, appId]);
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
  // A trial tried without a window is not kept to read with, so it is tried again once the window is captured.
  const check = useLiveCheck(trial, `${edit.again ?? ''}, ${windowless === null ? 'window' : 'no window'}`);
  const dropFixes = agreedFixes(check.preview, edit.lost);
  // The size of the screenshot the region would show over, when an edited value was drawn at another: its region
  // is in the pixels of the size it was drawn at, so it shows and moves on screenshots of that size only.
  const otherSize =
    edited !== null && editing === null && native !== undefined && !sameSize(native, edited.screen.windowSize)
      ? native
      : null;
  // The preview is for the color fields the inspector shows now.
  const colorFields = step === 'how' && fixing === null && (draft.kind === 'barFill' || draft.kind === 'color');
  // While a reference is taught, the box the screenshot shown has selected.
  const chosenOnShown = editing === null ? null : boxes.onShown;
  const preview = useMatchPreview(
    colorFields ? draft.kind : null,
    draft,
    selected,
    // An edited value's region is in the pixels of the size it was drawn at, and shows on no other.
    otherSize === null || edited === null
      ? null
      : `Select a screenshot at ${sizeText(edited.screen.windowSize)} to see the matching pixels of its region.`,
  );
  const previewField = <MatchPreviewField preview={preview} />;
  // An edited value's region sits on a screenshot of the size it was drawn at, and a bar is looked for on those.
  const detectBlocked =
    windowless ??
    (edited !== null && draft.drawnOn === null
      ? `Take a screenshot with the window at ${sizeText(edited.screen.windowSize)} to detect the bar: its region was drawn at that size.`
      : null);
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
          windowless ??
          (otherSize !== null && edited !== null
            ? `Recording needs the window at ${sizeText(edited.screen.windowSize)}, the size ${edited.name} was drawn at.`
            : lookBlocked(draft))
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
  // A new number that needs nothing but a reference saves as a draft, which the Values page lists until its
  // application has a reference it can read with.
  const keepsWithoutReference = mode.kind === 'value' && keptForReference(hasSide, draft.drawnOn, missing);
  // **Save and make a reference** is offered where the application has no reference to pick.
  const offersReference = onMakeReference !== null && file?.references.length === 0;
  // Saves a reference's draft, starting from the value's screenshot and region, then the value's draft beside it,
  // and opens the reference's. A value whose draft fails to save takes the reference's draft back.
  const makeReference = async (): Promise<void> => {
    const reference = referenceForValue(appId, setup);
    if (onMakeReference === null || reference === null) {
      return;
    }
    setSaving(true);
    try {
      const saved = await saveBeside(reference, drafting.save);
      if (saved !== null) {
        onMakeReference(saved);
      }
    } catch (error: unknown) {
      setSaveError(errorText(error));
    } finally {
      setSaving(false);
    }
  };
  const whereDone = named && draft.region !== null;
  const fixName = fixing === null ? null : `Fix ${fixing.name}`;
  const flowName = fixName ?? (edited === null ? 'New value' : `Edit ${edited.name}`);
  // The saved reference a fix of a number shares with its first drawing.
  const fixedCheck = fixing?.screen.check;
  const sharedReference = fixedCheck?.kind === 'number' ? fixedCheck.reference : null;
  // A number's characters and its check are steps of their own; other kinds read and save in their second step.
  const threeSteps = draft.kind === 'number' && fixing === null;
  const shownStep: Step = !threeSteps && step === 'check' ? 'how' : step;
  // The value's place is drawn over the live window, and the press takes the screenshot it goes on.
  const liveFrame = liveDrawFrame({
    settingPlace: shownStep === 'where' && editing === null,
    live: selected === null ? liveSize : null,
    occupied: recordingNow !== null || sheeted || picking !== null,
  });
  const regionOnLive = liveFrame === null ? null : regionOverFrame(draft.region, regionSize, liveFrame);
  const regionShown = regionOnStage(shownStep, draft.kind, recordingNow !== null);
  const drawing: Drawing = liveFrame !== null ? 'live' : editing !== null ? 'boxes' : regionShown ? 'region' : 'none';
  // While the value's place is set, the strip holds the one screenshot it is on, or the one shown.
  const placeShot = draft.drawnOn ?? selected;
  const stripShots = setsPlace ? shots.filter((shot) => shot.index === placeShot) : shots;
  // Where to read lists one screenshot, so entering it shows that one.
  const goTo = (next: Step): void => {
    setStep(next);
    setSelected(shownOnStep(next, draft.drawnOn, selected));
  };
  const stepper = (
    <Stepper
      tabs={stepTabs(threeSteps)}
      current={shownStep}
      done={(tab) => (tab === 'where' ? whereDone : tab === 'how' ? threeSteps && draft.reference !== null : false)}
      onStep={goTo}
    />
  );
  // A number reads with one of its application's saved references, which its second step picks.
  const referencePicker = (
    <ReferencePicker
      appName={choice.name}
      file={file}
      selected={draft.reference}
      whole={draft.whole}
      characters={draft.characters}
      onSelect={(reference) => {
        setDraft((current) => ({ ...current, reference }));
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
          onSelect={(reference) => {
            setDraft((current) => ({ ...current, reference }));
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

  return (
    <main className="capture" ref={page}>
      <ScreenshotStage
        stage={stage}
        label="Capture"
        bar={{
          path: [
            ...trail,
            ...(sheeted ? [teaching.name] : []),
            ...(checking && reviewed !== null ? ['Review recording'] : []),
          ],
          current: checking
            ? 'Check characters'
            : reviewed !== null
              ? 'Review recording'
              : editing === null
                ? flowName
                : teaching.name,
          status:
            recordingNow !== null
              ? { kind: 'recording', elapsedMs: recordingNow.elapsedMs }
              : checking
                ? { kind: 'checking', step: teaching.checkStep }
                : reviewed !== null
                  ? { kind: 'reviewing', recording: reviewed }
                  : absent === null
                    ? { kind: 'live', appName: choice.name }
                    : { kind: 'absent' },
          size: sheeted ? null : (native ?? null),
        }}
        actions={
          absent === null ? (
            <ChangeWindow
              busy={busy}
              // Finishing a draft starts on its screenshots, which the new window leaves behind.
              loses={holds === 'unsaved' ? 'unsaved' : resumed === null ? 'nothing' : 'draft'}
              onChangeWindow={onChangeWindow}
            />
          ) : null
        }
        absent={absent}
        onLiveFailed={onWindowFailed}
        onTake={takeShot}
        sheet={teachingSheet(teaching)}
        notes={
          <>
            {windowNotes}
            {selection.error === null ? null : (
              <p className="warn" role="alert">
                {selection.error}
              </p>
            )}
          </>
        }
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
            drawnAt: edited === null ? null : sizeText(edited.screen.windowSize),
            boxBlock: teaching.boxBlock,
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
            {liveFrame !== null ? (
              <RegionSurface
                size={liveFrame}
                shown={shown}
                screenshot={null}
                // The box drawn stays outlined from when the pointer is let go until its screenshot arrives, and a
                // region placed at the live frame's size shows over it until a press.
                region={live.shown.frozen ? (live.shown.box?.rect ?? null) : regionOnLive}
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
                  live.drew({ rect, at: liveFrame });
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
                  editable={teaching.boxBlock === null || picking !== null}
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
                region={regionShown && otherSize === null ? draft.region : null}
                recording={recordingNow !== null && selected === null}
                area={area}
                label={regionLabel(valueName, moving)}
                editable={shownPicture !== null && (picking !== null || (regionShown && otherSize === null))}
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
              {saveError === null ? null : (
                <p className="warn" role="alert">
                  {saveError}
                </p>
              )}
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
              {appFile.error === null ? null : (
                <p className="warn" role="alert">
                  {appFile.error}
                </p>
              )}
              {offersReference ? (
                <div>
                  <p className="note">
                    Save and make a reference keeps this value as a draft and opens a new reference, starting from this
                    screenshot and region. Once the reference is saved, finish the value on the Values page.
                  </p>
                  {keepsWithoutReference ? null : <p className="note">{missing[0]}</p>}
                  <button
                    type="button"
                    className="btn"
                    aria-busy={saving}
                    disabled={!keepsWithoutReference || busy}
                    onClick={() => {
                      background(makeReference());
                    }}
                  >
                    Save and make a reference
                  </button>
                </div>
              ) : null}
              <div className="sep" />
              {saveError === null ? null : (
                <p className="warn" role="alert">
                  {saveError}
                </p>
              )}
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
              {appFile.error === null ? null : (
                <p className="warn" role="alert">
                  {appFile.error}
                </p>
              )}
              {lookPanel}
              {check.state.kind === 'set' && check.preview !== null && check.preview.unread !== null ? (
                <p className="reading note">{check.preview.unread}</p>
              ) : (
                <ReadingBox
                  built={trial.ok ? null : trial.next}
                  state={check.state}
                  reads={check.reads}
                  windowless={windowless}
                />
              )}
              {edit.built === null || check.preview === null ? null : (
                <EditOutcome preview={check.preview} agreed={dropFixes.length > 0} onAgree={edit.agree} />
              )}
              {/* Keyed by the check's kind, so scores of the last kind never show under a new one. */}
              <ReadingDetailsBox key={builtKind} ask={check.state.kind === 'set' ? explainReading : null} />
              <div className="sep" />
              {saveError === null ? null : (
                <p className="warn" role="alert">
                  {saveError}
                </p>
              )}
              <p className="note">
                {fixing !== null
                  ? `The fix shows on the Values page, beside ${fixing.name}.`
                  : edited !== null
                    ? `The changes to ${edited.name} show on the Values page, and its automations keep using it.`
                    : keepsWithoutReference
                      ? `No reference is picked, so Save value keeps this value as a draft. The Values page lists it under ${choice.name} until it has a reference to read with.${trail.length > 1 ? ' The automation shows again without it.' : ''}`
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
                  disabled={!canSave && !(keepsWithoutReference && !busy)}
                  onClick={() => {
                    background(keepsWithoutReference ? drafting.saveAndClose() : save());
                  }}
                >
                  {fixing !== null ? 'Save fix' : edited === null ? 'Save value' : 'Save changes'}
                </button>
              </div>
            </>
          )}
        </aside>
      ) : (
        <ReferenceInspector
          teaching={teaching}
          editing={editing}
          setEditing={setEditing}
          draft={draft}
          setDraft={setDraft}
          boxes={boxes}
          appName={choice.name}
          file={file}
          full={full}
          position={position}
          setSelected={setSelected}
          saving={referenceSave ?? NO_REFERENCE_SAVE}
          check={check}
          windowless={windowless}
          busy={busy}
          draftError={draftError}
          draftButton={draftButton}
          onDiscard={onCancel}
        />
      )}
    </main>
  );
}
