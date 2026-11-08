import { useCallback, useMemo, useRef, useState } from 'react';

import { type ReferenceDef, type ReferenceRef, errorText, saveReference, saveValue } from '../lib/api.ts';
import { LiveCanvas, PictureCanvas } from './canvas.tsx';
import {
  type Built,
  type Draft,
  buildCharacters,
  buildSpec,
  CHECK_KINDS,
  type Drawn,
  EMPTY_DRAFT,
  fromReference,
  KIND_NAMES,
  newestOfSize,
  recordBlocked,
  regionScreenshot,
  settingsDiffer,
  toReference,
  withKind,
} from '../lib/check.ts';
import { readers } from '../lib/reference.ts';
import { type ColorTarget, ColorFields, SelectFields } from './CheckFields.tsx';
import { ReadingBox, RegionFields } from './Fields.tsx';
import { Question, useQuestion } from './Question.tsx';
import { ReferenceEditor } from './ReferenceEditor.tsx';
import { ReferencePicker } from './ReferencePicker.tsx';
import type { Choice } from './Picker.tsx';
import { colorAt } from '../lib/pixels.ts';
import { RecordPanel, ReviewSheet } from './Recording.tsx';
import { clock } from '../lib/review.ts';
import { type Point, type Size, fit } from '../lib/region.ts';
import { RegionSurface } from './RegionSurface.tsx';
import type { SelectTool } from '../lib/selection.ts';
import { StatesField } from './StatesField.tsx';
import { background } from '../lib/repeat.ts';
import { useSize } from '../lib/size.ts';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';
import { useAppFile } from '../lib/useAppFile.ts';
import { useLiveCheck } from '../lib/useLiveCheck.ts';
import { useRecording } from '../lib/useRecording.ts';
import { useReview } from '../lib/useReview.ts';
import { useScreenshotShortcut } from '../lib/useScreenshotShortcut.ts';
import { useScreenshots } from '../lib/useScreenshots.ts';
import { useStateSelections } from '../lib/useStateSelections.ts';

/** The widest live frame asked for, which keeps a large window's preview cheap. */
const LIVE_MAX_WIDTH = 1600;

function viewHint(
  recording: boolean,
  selected: number | null,
  shotCount: number,
  picking: boolean,
  liveError: string | null,
  shortcut: string,
  tool: SelectTool,
): string {
  if (selected === null && liveError !== null) {
    return liveError;
  }
  if (recording && selected === null) {
    return 'Recording. Use the application; each different reading is kept.';
  }
  if (selected === null) {
    return shotCount === 0
      ? `Take a screenshot (${shortcut}) when the window shows what to read.`
      : 'Select a screenshot to draw on it.';
  }
  if (picking) {
    return 'Click the color to pick it.';
  }
  if (tool === 'snap') {
    return 'Drag a rough box around the thing to snap to it.';
  }
  if (tool === 'click') {
    return 'Click the thing to select it.';
  }
  return 'Drag to draw a region. Drag inside it to move it, or drag a corner to resize it.';
}

/** What the capture screen makes: a value, or a reference, new or saved, on the References page. */
export type CaptureMode =
  | { readonly kind: 'value' }
  | { readonly kind: 'reference'; readonly existing: ReferenceDef | null };

/** The reference being made or changed, and its name as typed. */
interface Editing {
  readonly existing: ReferenceDef | null;
  readonly name: string;
  /** The value's region when the editor opened from a value, to put back when it closes. */
  readonly valueRegion: Drawn;
}

/** The value's two steps: where it shows, then how to read it. */
type Step = 'where' | 'how';

interface Props {
  readonly choice: Choice;
  readonly mode: CaptureMode;
  readonly onCancel: () => void;
  readonly onChangeWindow: () => void;
  readonly onSaved: () => void;
  /** Hears what leaving the capture would lose. */
  readonly onHeld: (held: Held) => void;
}

export function Capture({ choice, mode, onCancel, onChangeWindow, onSaved, onHeld }: Props) {
  // What the capture shows before the user does anything, which leaving it loses nothing of.
  const [start] = useState(() => {
    const editing: Editing | null =
      mode.kind === 'reference'
        ? { existing: mode.existing, name: mode.existing?.name ?? '', valueRegion: { region: null, drawnOn: null } }
        : null;
    return {
      screenshots: 0,
      recording: false,
      valueName: '',
      editing,
      draft: editing === null ? EMPTY_DRAFT : toReference(EMPTY_DRAFT, editing.existing),
    };
  });
  const [draft, setDraft] = useState<Draft>(start.draft);
  // The saved application the value or reference belongs to.
  const appId = choice.app;
  const appFile = useAppFile(appId);
  const { file } = appFile;
  const [step, setStep] = useState<Step>('where');
  const [editing, setEditing] = useState<Editing | null>(start.editing);
  const [picking, setPicking] = useState<ColorTarget | null>(null);
  const [tool, setTool] = useState<SelectTool>('box');
  const [valueName, setValueName] = useState(start.valueName);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const screenshots = useScreenshots();
  const { shots, full, selected, setSelected, position } = screenshots;
  const shortcut = useScreenshotShortcut(screenshots.take);
  const recorder = useRecording();
  const recording = recorder.view;

  const holds = held(saving, start, {
    screenshots: shots.length,
    recording: recording !== null,
    valueName,
    editing,
    draft,
  });
  useReportHeld(onHeld, holds);
  const [leavingWindow, setLeavingWindow] = useState(false);
  const leaveQuestion = useQuestion<HTMLButtonElement>(leavingWindow);

  const view = useRef<HTMLDivElement>(null);
  const space = useSize(view);

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
  const check = useLiveCheck(built);
  const selection = useStateSelections(draft, setDraft, selected);
  const selecting = draft.kind === 'state' ? tool : 'box';
  const review = useReview(recording, draft, setDraft, editingRef);
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

  const shownOn = regionScreenshot(selected, draft.drawnOn, full, shots.at(-1)?.index ?? null);
  const native = shownOn === null ? undefined : full.get(shownOn);
  // The window size the region's pixels refer to.
  const drawnPicture = draft.drawnOn === null ? undefined : full.get(draft.drawnOn);
  const drawnSize = drawnPicture === undefined ? null : { width: drawnPicture.width, height: drawnPicture.height };
  const shownPicture = selected === null ? null : (full.get(selected) ?? null);
  const layoutSize: Size | null = selected === null ? liveSize : shownPicture;
  const shown = layoutSize === null ? { width: 0, height: 0 } : fit(layoutSize, space);
  const frameStyle = {
    left: (space.width - shown.width) / 2,
    top: (space.height - shown.height) / 2,
    width: shown.width,
    height: shown.height,
  };

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

  const save = async (): Promise<void> => {
    if (!built.ok || draft.drawnOn === null) {
      return;
    }
    setSaving(true);
    try {
      await saveValue({
        app: choice.app,
        valueName: valueName.trim(),
        check: built.spec,
        screenshot: draft.drawnOn,
      });
      onSaved();
    } catch (error: unknown) {
      setSaveError(errorText(error));
    } finally {
      setSaving(false);
    }
  };

  const recordingBlocked = editing === null ? null : recordBlocked(draft);

  // Starts a new recording, or with `more` goes on with the stopped one.
  const record = async (more: boolean): Promise<void> => {
    const { region, ink, tolerance, drawnOn } = draft;
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

  // Opens the reference editor on `existing`, or on a new reference read
  // from the value's region.
  const openEditor = (existing: ReferenceDef | null): void => {
    setEditing({ existing, name: existing?.name ?? '', valueRegion: { region: draft.region, drawnOn: draft.drawnOn } });
    setDraft((current) => toReference(current, existing));
    setPicking(null);
    setSaveError(null);
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
      });
      setSaveError(null);
      if (mode.kind === 'reference') {
        onSaved();
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

  const canSave =
    valueName.trim().length > 0 && built.ok && draft.drawnOn !== null && check.state.kind === 'set' && !saving;
  const canSaveRef =
    editing !== null && editing.name.trim().length > 0 && built.ok && check.state.kind === 'set' && !saving;
  const whereDone = valueName.trim().length > 0 && draft.region !== null;
  const editedName = editing === null || editing.name.trim() === '' ? 'New reference' : editing.name.trim();

  return (
    <main className="capture">
      <section className="stage" aria-label="Capture">
        <div className="bar">
          <span className="crumb">
            {mode.kind === 'reference' ? 'References' : 'Values'}
            {mode.kind === 'value' && editing !== null ? ' / New value' : ''} /{' '}
            {reviewing ? (
              <>
                {editedName} / <b>Review recording</b>
              </>
            ) : (
              <b>{editing === null ? 'New value' : editedName}</b>
            )}
          </span>
          {recording?.recording === true ? (
            <span className="pill recording">
              <span className="dot" />
              Recording {clock(recording.elapsedMs)}
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
                aria-describedby={review.keepBlocked === null ? undefined : 'keep-blocked'}
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
                  busy={saving}
                  onKeep={() => {
                    setLeavingWindow(false);
                  }}
                  onAct={onChangeWindow}
                >
                  Change window? The screenshots, everything set up on them, and any recording are lost.
                </Question>
              ) : (
                <button
                  ref={leaveQuestion.control}
                  type="button"
                  className="btn"
                  disabled={saving}
                  onClick={() => {
                    if (holds === 'unsaved') {
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
                {shortcut.choosing ? 'Press the new shortcut, or Esc' : 'Change shortcut'}
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
        {shortcut.error === null ? null : <p className="warn">{shortcut.error}</p>}
        {screenshots.error === null ? null : <p className="warn">{screenshots.error}</p>}
        {reviewing && recording !== null ? (
          <ReviewSheet
            view={recording}
            entries={review.entries}
            problems={review.flagged}
            crops={recorder.crops}
            blocked={review.keepBlocked}
            error={recordError}
            onEntry={review.edit}
            onKeepAll={review.keepAll}
          />
        ) : null}
        {/* Hidden rather than removed during review, so the view keeps its measured size. */}
        <div className="view" ref={view} hidden={reviewing}>
          <div className="frame" style={frameStyle}>
            {reviewing ? null : selected === null ? (
              <LiveCanvas
                maxWidth={Math.min(LIVE_MAX_WIDTH, Math.max(1, Math.round(space.width * devicePixelRatio)))}
                onFrame={onFrame}
                onError={onLiveError}
              />
            ) : shownPicture === null ? null : (
              <PictureCanvas picture={shownPicture} label={`Screenshot ${position(selected)}`} />
            )}
            {native === undefined ? null : (
              <RegionSurface
                size={native}
                shown={shown}
                region={draft.region}
                recording={recording?.recording === true && selected === null}
                label={editing !== null ? editedName : valueName.trim() === '' ? 'Region' : valueName.trim()}
                editable={shownPicture !== null}
                picking={picking !== null}
                tool={selecting}
                ignored={selection.ignored}
                onRegion={(region) => {
                  setDraft((current) => ({ ...current, region, drawnOn: selected }));
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
          </div>
          <p className="hint">
            {viewHint(
              recording?.recording === true,
              selected,
              shots.length,
              picking !== null,
              liveError,
              shortcut.label,
              selecting,
            )}
          </p>
        </div>
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
          {shots.map((shot, position_) => (
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
              {position_ + 1} · {shot.time}
            </button>
          ))}
          <span className="note">
            Screenshots last until you change the window.
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
          {step === 'where' ? (
            <>
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
                <span id="check-kind">Check</span>
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
                  onTool={(next) => {
                    setTool(next);
                    setPicking(null);
                  }}
                  onColorMatch={(value) => {
                    setDraft((current) => ({ ...current, colorMatch: value }));
                  }}
                />
              ) : null}
              {selection.error === null ? null : <p className="warn">{selection.error}</p>}
              <RegionFields
                region={draft.region}
                onRegion={(region) => {
                  // Typed pixels keep to the screenshot the region was drawn on.
                  setDraft((current) => ({ ...current, region, drawnOn: current.drawnOn ?? shownOn }));
                }}
              />
              <p className="note">
                Draw the region where the value shows, on any screenshot of {choice.name}. Only where it is matters
                here; how to read it comes next.
              </p>
              <div className="sep" />
              <div className="actions">
                <button type="button" className="btn" disabled={saving} onClick={onCancel}>
                  Discard
                </button>
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
              {draft.kind === 'number' ? (
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
                    openEditor(file?.references.find((reference) => reference.id === id) ?? null);
                  }}
                  onNew={() => {
                    openEditor(null);
                  }}
                  onWhole={(whole) => {
                    setDraft((current) => ({ ...current, whole }));
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
                />
              ) : (
                <ColorFields
                  draft={draft}
                  picking={picking}
                  canPick={shownPicture !== null}
                  onDraft={setDraft}
                  onPicking={setPicking}
                />
              )}
              {appFile.error === null ? null : <p className="warn">{appFile.error}</p>}
              <ReadingBox built={built.ok ? null : built.next} state={check.state} reading={check.reading} />
              <div className="sep" />
              {saveError === null ? null : <p className="warn">{saveError}</p>}
              <p className="note">Saved values show on the Values page, under {choice.name}.</p>
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
                <button
                  type="button"
                  className="btn primary"
                  disabled={!canSave}
                  onClick={() => {
                    background(save());
                  }}
                >
                  Save value
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
            saved={editing.existing?.samples.map((sample) => sample.text) ?? []}
            labeled={[...draft.labels.values()].filter((text) => text.trim() !== '')}
            recorded={[...draft.recorded.values()]}
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
            record={
              <RecordPanel
                view={recording}
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
          <ReadingBox built={built.ok ? null : built.next} state={check.state} reading={check.reading} />
          <div className="sep" />
          {saveError === null ? null : <p className="warn">{saveError}</p>}
          <div className="actions">
            <button
              type="button"
              className="btn"
              disabled={saving}
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
