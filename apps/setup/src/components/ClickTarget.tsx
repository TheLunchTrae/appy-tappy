import { type PointerEvent, useEffect, useReducer, useRef, useState } from 'react';

import { addClickPicture, clickNow, errorText, findClick, pickWindow, readWindow, select } from '../lib/api.ts';
import {
  type AppSummary,
  type ClickAtDef,
  type ClickButtonDef,
  type ClickDeliveryDef,
  LIMITS,
  type SelectionSpec,
} from '../lib/bindings.ts';
import {
  BUTTONS,
  BUTTON_NAMES,
  DEFAULT_TOLERANCE,
  DELIVERIES,
  DELIVERY_NAMES,
  DELIVERY_NOTES,
  DELIVERY_WORDS,
  SEARCH_MARGIN,
  SEARCH_SIDE,
  clickApps,
  clickBlocked,
  middle,
  placeSpot,
  returnsFocus,
  searchArea,
  sentText,
  spotAt,
} from '../lib/click.ts';
import { IDLE, tryStep } from '../lib/clickTry.ts';
import { type Point, type Rect, type Size, contains, pixelAt, toPicture } from '../lib/region.ts';
import { useCapture } from '../lib/capture.ts';
import { background } from '../lib/repeat.ts';
import { useNewest } from '../lib/request.ts';
import { DEFAULT_COLOR_MATCH, type SelectTool } from '../lib/selection.ts';
import { ScreenshotStage, useScreenshotStage } from './capture/ScreenshotStage.tsx';
import { SelectFields } from './CheckFields.tsx';
import { ClickPictureThumb } from './ClickPictureThumb.tsx';
import { Hint } from './Hint.tsx';
import { Marquee, RegionSurface } from './RegionSurface.tsx';

/** A click's target as the automation editor keeps it. */
interface ClickTargetValue {
  readonly app: string;
  readonly button: ClickButtonDef;
  readonly delivery: ClickDeliveryDef;
  readonly at: ClickAtDef;
}

/** Where a click looks: the same spot every time, or wherever its picture is. */
type Mode = 'spot' | 'picture';

/**
 * A picture drawn this session: its region on a screenshot, the selection that made it when the snap or click tool
 * did, and where on it the click presses.
 */
interface Drawn {
  readonly screenshot: number;
  readonly region: Rect;
  /** Only what this picks is compared; the rest of the region is left out. Null for a drawn box. */
  readonly selection: SelectionSpec | null;
  readonly press: Point;
}

/** What the picture step does with a press on the screenshot: mark the picture, or the place on it to press. */
type PictureStep = 'picture' | 'press';

/** Finding the picked window of the application the click is for. */
type Window =
  | { readonly kind: 'looking' }
  | { readonly kind: 'picked'; readonly handle: number }
  | { readonly kind: 'missing'; readonly message: string };

function hintText(
  window: Window,
  mode: Mode,
  step: PictureStep,
  selected: number | null,
  shortcut: string,
  tool: SelectTool,
): string {
  if (window.kind === 'missing') {
    return window.message;
  }
  if (window.kind === 'looking') {
    return 'Looking for the window…';
  }
  if (selected === null) {
    return `Take a screenshot (${shortcut}) when the window shows what to click.`;
  }
  if (mode === 'spot') {
    return 'Click where the click should land.';
  }
  if (step === 'press') {
    return 'Click inside the picture where the click should land.';
  }
  if (tool === 'snap') {
    return 'Drag a rough box around the button. Then click inside it where the click should land.';
  }
  if (tool === 'click') {
    return 'Click the button to select it. Then click inside it where the click should land.';
  }
  return 'Drag a box around the button. Then click inside it where the click should land.';
}

interface StageProps {
  readonly apps: readonly AppSummary[];
  readonly app: string;
  readonly appName: string;
  readonly automation: string;
  /** The target's place before this screen opened, kept until a new one is set up. */
  readonly initial: ClickAtDef | null;
  readonly button: ClickButtonDef;
  readonly delivery: ClickDeliveryDef;
  /** Whether a click with the cursor gives the focus back after, for Click it now. */
  readonly returnFocus: boolean;
  /** The ID of the application whose value the automation watches, or null before it watches one. */
  readonly watched: string | null;
  readonly tolerance: number;
  /** Picks the application the click happens in, which opens the screen again on it. */
  readonly onApp: (app: string) => void;
  /** Opens the screen again on the application, to find its window anew. */
  readonly onLookAgain: () => void;
  readonly onButton: (button: ClickButtonDef) => void;
  readonly onDelivery: (delivery: ClickDeliveryDef) => void;
  readonly onTolerance: (tolerance: number) => void;
  readonly onCancel: () => void;
  readonly onUse: (target: ClickTargetValue) => void;
}

function TargetStage({
  apps,
  app,
  appName,
  automation,
  initial,
  button,
  delivery,
  returnFocus,
  watched,
  tolerance,
  onApp,
  onLookAgain,
  onButton,
  onDelivery,
  onTolerance,
  onCancel,
  onUse,
}: StageProps) {
  const [window, setWindow] = useState<Window>({ kind: 'looking' });
  const [mode, setMode] = useState<Mode>(initial?.kind ?? 'picture');
  const [spot, setSpot] = useState<ClickAtDef | null>(initial?.kind === 'spot' ? initial : null);
  const [drawn, setDrawn] = useState<Drawn | null>(null);
  const [step, setStep] = useState<PictureStep>('picture');
  const [tool, setTool] = useState<SelectTool>('box');
  const [colorMatch, setColorMatch] = useState(DEFAULT_COLOR_MATCH);
  const selecting = useNewest();
  // What Find it now and Click it now are doing or answered. Each try has a number, so a late answer is dropped.
  const [tried, tryNext] = useReducer(tryStep, IDLE);
  const attempts = useRef(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The picture kept for a drawn region, so finding and using it keep it once.
  const kept = useRef<{ readonly key: string; readonly at: ClickAtDef } | null>(null);
  // The stage's one pick of its window. What finishes after the stage closed, a pick or a target, is not used.
  const picks = useNewest();
  // The stage holds the capture until it goes: when the screen closes, or another stage opens, which may find no
  // window to pick in its place. Its screenshots go with it.
  useCapture();

  // What trying the target answered shows below the buttons, which may be past the end of a short window.
  const answer = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (tried.kind === 'answered' || tried.kind === 'failed') {
      answer.current?.scrollIntoView({ block: 'nearest' });
    }
  }, [tried]);

  const stage = useScreenshotStage();
  const { liveError, shownPicture } = stage;
  const { shots, full, selected, setSelected } = stage.screenshots;

  const choices = clickApps(apps);
  const file = apps.find((candidate) => candidate.id === app)?.file ?? null;
  // The application's file as the lists last read it, which the window is picked by.
  const latestFile = useRef(file);
  useEffect(() => {
    latestFile.current = file;
  });

  // Picks the window the runtime clicks in, which it finds by the application's rules without the fallback.
  useEffect(() => {
    const isOpen = picks.begin();
    const file = latestFile.current;
    if (file === null) {
      setWindow({ kind: 'missing', message: 'Pick an application to click in.' });
      return undefined;
    }
    setWindow({ kind: 'looking' });
    background(
      (async (): Promise<void> => {
        try {
          const read = await readWindow({ ...file.window, fallback: 'none' });
          if (read === null) {
            if (isOpen()) {
              setWindow({
                kind: 'missing',
                message: `No open window matches ${file.name}'s window rules. Open it, then look again.`,
              });
            }
            return;
          }
          // A screen closed meanwhile has stopped capture, which a pick now would start again.
          if (!isOpen()) {
            return;
          }
          await pickWindow(read);
          if (isOpen()) {
            setWindow({ kind: 'picked', handle: read });
          }
        } catch (caught: unknown) {
          if (isOpen()) {
            setWindow({ kind: 'missing', message: errorText(caught) });
          }
        }
      })(),
    );
    return undefined;
    // The window rules are read when the stage opens, and looking again opens a new one, not each time the lists
    // are read again.
  }, [picks]);

  // The window's own size: a screenshot's, since the live view may be scaled down.
  const newest = shots.at(-1);
  const native = shownPicture ?? (newest === undefined ? null : (full.get(newest.index) ?? null));
  const nativeSize: Size | null = native === null ? null : { width: native.width, height: native.height };

  const drawnHere = drawn?.screenshot === selected ? drawn : null;
  const pictureSize = shownPicture === null ? null : { width: shownPicture.width, height: shownPicture.height };
  const area = drawnHere === null || pictureSize === null ? null : searchArea(drawnHere.region, pictureSize);

  // A click that is being sent is a real click, so the target stays as it is until the click's answer shows.
  const sending = tried.kind === 'clicking';

  const changed = (): void => {
    tryNext({ kind: 'changed', what: 'place' });
    setError(null);
  };

  const setRegion = (region: Rect, selection: SelectionSpec | null): void => {
    if (selected === null || sending) {
      return;
    }
    changed();
    setDrawn({ screenshot: selected, region, selection, press: middle(region) });
  };

  const selectWith = (steps: SelectionSpec['steps']): void => {
    if (selected === null || sending) {
      return;
    }
    const on = selected;
    const selection: SelectionSpec = { tolerance: colorMatch, steps };
    // A selection that a newer one overtook marks nothing.
    const current = selecting.begin();
    background(
      select(on, selection)
        .then((view_) => {
          if (!current()) {
            return;
          }
          if (view_ === null) {
            setError('Nothing was selected there. Try a bigger box or a higher color match.');
            return;
          }
          setRegion(view_.region, selection);
          setStep('press');
        })
        .catch((caught: unknown) => {
          if (current()) {
            setError(errorText(caught));
          }
        }),
    );
  };

  // A press on the screenshot while a spot or the place on the picture is picked.
  const pointAt = (event: PointerEvent<HTMLDivElement>): Point | null => {
    if (pictureSize === null || event.button !== 0) {
      return null;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const point = toPicture(
      { x: event.clientX - bounds.left, y: event.clientY - bounds.top },
      { width: bounds.width, height: bounds.height },
      pictureSize,
    );
    return pixelAt(point, pictureSize);
  };
  const onPoint = (event: PointerEvent<HTMLDivElement>): void => {
    const point = pointAt(event);
    if (point === null || pictureSize === null || sending) {
      return;
    }
    if (mode === 'spot') {
      changed();
      setSpot(spotAt(point, pictureSize));
      return;
    }
    if (drawnHere !== null && contains(drawnHere.region, point)) {
      changed();
      setDrawn({ ...drawnHere, press: { x: point.x - drawnHere.region.x, y: point.y - drawnHere.region.y } });
    }
  };

  /**
   * The target's place as set up now, keeping a newly drawn picture first, or why there is none. A failed command
   * rejects with its message, as every command does.
   */
  const target = async (): Promise<ClickAtDef | { readonly problem: string }> => {
    const unset = {
      problem: mode === 'spot' ? 'Click where the click should land first.' : 'Draw the picture to click on first.',
    };
    if (mode === 'spot') {
      return spot ?? unset;
    }
    if (drawn === null) {
      return initial?.kind === 'picture' ? { ...initial, tolerance } : unset;
    }
    const picture = full.get(drawn.screenshot);
    if (picture === undefined) {
      return unset;
    }
    const size = { width: picture.width, height: picture.height };
    const search = searchArea(drawn.region, size);
    if (search === null) {
      return {
        problem: `The picture is too big to look for. Draw it smaller: at most ${LIMITS.maxSearchPixels.toLocaleString()} pixels.`,
      };
    }
    const { region, selection } = drawn;
    const key = JSON.stringify([drawn.screenshot, region, selection]);
    let base = kept.current?.key === key ? kept.current.at : null;
    if (base === null) {
      const view_ = await addClickPicture(drawn.screenshot, region, selection);
      base = {
        kind: 'picture',
        image: view_.image,
        windowSize: { width: view_.windowWidth, height: view_.windowHeight },
        search,
        press: drawn.press,
        tolerance,
      };
      kept.current = { key, at: base };
    }
    return base.kind === 'picture' ? { ...base, search, press: drawn.press, tolerance } : base;
  };

  /** Runs `task` on the target as set up now, and hands `problem` why when there is no target to run it on. */
  const run = (task: (at: ClickAtDef) => Promise<void> | void, problem: (message: string) => void = setError): void => {
    setBusy(true);
    setError(null);
    background(
      (async (): Promise<void> => {
        try {
          const at = await target();
          if ('problem' in at) {
            problem(at.problem);
            return;
          }
          await task(at);
        } catch (caught: unknown) {
          problem(errorText(caught));
        } finally {
          setBusy(false);
        }
      })(),
    );
  };

  const nextAttempt = (): number => {
    attempts.current += 1;
    return attempts.current;
  };

  // A try starts with the press, and every way it can end answers it, so none stays under way.
  const findNow = (): void => {
    const attempt = nextAttempt();
    const failed = (message: string): void => {
      tryNext({ kind: 'failed', attempt, message });
    };
    tryNext({ kind: 'find', attempt });
    run(async (at) => {
      try {
        tryNext({ kind: 'answered', attempt, answer: { kind: 'found', at: await findClick(at) } });
      } catch (caught: unknown) {
        failed(errorText(caught));
      }
      // The result shows on the live window.
      setSelected(null);
    }, failed);
  };

  const use = (): void => {
    const isOpen = picks.join();
    run((at) => {
      if (isOpen()) {
        onUse({ app, button, delivery, at });
      }
    });
  };

  // Anti-cheat can see a click however it is sent, so a test click needs what the automation's click needs.
  const named = (id: string) => {
    const file = apps.find((candidate) => candidate.id === id)?.file ?? null;
    return file === null ? null : { name: file.name, antiCheat: file.antiCheat };
  };
  const clickedIn = named(app);
  const blocked =
    clickedIn === null ? null : clickBlocked(watched === null ? clickedIn : (named(watched) ?? clickedIn), clickedIn);

  // Sends one real click. Only the Click it now button calls it, and not while anti-cheat blocks the click.
  const clickItNow = (): void => {
    if (blocked !== null || window.kind !== 'picked') {
      return;
    }
    const shown = window.handle;
    const attempt = nextAttempt();
    const failed = (message: string): void => {
      tryNext({ kind: 'failed', attempt, message });
    };
    // From here until its answer, the button, the way to click, and the place cannot change.
    tryNext({ kind: 'click', attempt });
    run(async (at) => {
      try {
        const sent = await clickNow({ watched, app, button, delivery, returnFocus, at });
        const elsewhere = sent.window !== shown;
        tryNext({
          kind: 'answered',
          attempt,
          answer: { kind: 'sent', message: sentText(delivery, appName, sent.title, elsewhere), elsewhere },
        });
      } catch (caught: unknown) {
        failed(errorText(caught));
      }
    }, failed);
  };
  const landed = tried.kind === 'answered' && tried.answer.kind === 'found' ? tried.answer.at : null;
  const elsewhere = tried.kind === 'answered' && tried.answer.kind === 'sent' && tried.answer.elsewhere;

  const spotHere = spot?.kind === 'spot' && pictureSize !== null ? placeSpot(spot, pictureSize) : null;
  const ready = mode === 'spot' ? spot !== null : drawn !== null || initial?.kind === 'picture';
  const pointing = shownPicture !== null && (mode === 'spot' || (step === 'press' && drawnHere !== null));

  return (
    <main className="capture">
      <ScreenshotStage
        stage={stage}
        label="Click target"
        bar={{
          path: ['Automations', automation],
          current: 'Click target',
          status: { kind: 'live', appName },
          size: native,
        }}
        actions={
          window.kind === 'missing' || (window.kind === 'picked' && liveError !== null) || elsewhere ? (
            <button
              type="button"
              className="btn"
              disabled={busy}
              title="Finds the application's window again. Screenshots taken so far are cleared."
              onClick={onLookAgain}
            >
              Look again
            </button>
          ) : null
        }
        watching={window.kind === 'picked'}
        hint={(shortcut) =>
          selected === null && liveError !== null ? liveError : hintText(window, mode, step, selected, shortcut, tool)
        }
        strip={{
          shots: null,
          removal: null,
          note: 'Screenshots last until you change the application.',
          onSelect: null,
        }}
      >
        {({ shown }) => {
          const scale = nativeSize === null || nativeSize.width === 0 ? 0 : shown.width / nativeSize.width;
          const marker = (point: Point) => ({ left: (point.x + 0.5) * scale, top: (point.y + 0.5) * scale });
          return (
            <>
              {mode === 'picture' && area !== null ? (
                <div
                  className="search-area"
                  style={{
                    left: area.x * scale,
                    top: area.y * scale,
                    width: area.width * scale,
                    height: area.height * scale,
                  }}
                >
                  <span className="tag">
                    Search area · {area.width} × {area.height}
                  </span>
                </div>
              ) : null}
              {mode === 'picture' && shownPicture !== null && !pointing ? (
                <RegionSurface
                  size={shownPicture}
                  shown={shown}
                  screenshot={selected}
                  region={drawnHere?.region ?? null}
                  label="Picture"
                  editable={!sending}
                  tool={tool}
                  onRegion={(region) => {
                    setRegion(region, null);
                  }}
                  onSnap={(rough) => {
                    selectWith([{ kind: 'snap', region: rough, combine: 'replace' }]);
                  }}
                  onSelectAt={(point) => {
                    selectWith([{ kind: 'click', x: point.x, y: point.y, combine: 'replace' }]);
                  }}
                />
              ) : null}
              {pointing ? (
                <div className="surface clicking" onPointerDown={onPoint}>
                  {mode === 'picture' && drawnHere !== null && pictureSize !== null ? (
                    <Marquee rect={drawnHere.region} picture={pictureSize} scale={scale} label="Picture" />
                  ) : null}
                </div>
              ) : null}
              {mode === 'picture' && drawnHere !== null ? (
                <span
                  className="click-spot"
                  style={marker({
                    x: drawnHere.region.x + drawnHere.press.x,
                    y: drawnHere.region.y + drawnHere.press.y,
                  })}
                  aria-hidden="true"
                />
              ) : null}
              {mode === 'spot' && spotHere !== null ? (
                <span className="click-spot" style={marker(spotHere)} aria-hidden="true" />
              ) : null}
              {selected === null && landed !== null && nativeSize !== null ? (
                <span className="click-spot found" style={marker(landed)} aria-hidden="true" />
              ) : null}
            </>
          );
        }}
      </ScreenshotStage>
      <aside className="inspector" aria-label="Click target">
        <div className="field">
          <span className="name">
            <label htmlFor="click-app">Application</label>
            <Hint topic="Application" id="hint-click-app">
              The saved application the click happens in. AppyTappy finds its window with its window rules on the
              Applications page, and clicks only in a window that matches them.
            </Hint>
          </span>
          <select
            id="click-app"
            className="input"
            aria-describedby="hint-click-app"
            value={app}
            // The application stays while a picture is kept or looked for.
            disabled={busy}
            onChange={(event) => {
              onApp(event.target.value);
            }}
          >
            {app === '' ? (
              <option value="" disabled>
                Pick an application
              </option>
            ) : choices.some((choice) => choice.id === app) ? null : (
              <option value={app} disabled>
                {app} (no longer saved)
              </option>
            )}
            {choices.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.name}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <span className="name">
            <span id="click-where">Where to click</span>
            <Hint topic="Where to click" id="hint-click-where">
              A spot clicks the same place in the window every time. A picture clicks only once the picture is found, so
              a button that moved a little is still hit and a hidden one is never clicked by mistake.
            </Hint>
          </span>
          <div className="seg" role="group" aria-labelledby="click-where" aria-describedby="hint-click-where">
            {(['spot', 'picture'] as const).map((option) => (
              <button
                key={option}
                type="button"
                aria-pressed={mode === option}
                disabled={sending}
                onClick={() => {
                  changed();
                  setMode(option);
                }}
              >
                {option === 'spot' ? 'A spot' : 'A picture'}
                <small>{option === 'spot' ? 'Same place every time' : "Only when it's there"}</small>
              </button>
            ))}
          </div>
          <span className="note">
            {mode === 'spot'
              ? 'AppyTappy measures the spot from the window corner nearest it, so it stays put when the window is resized.'
              : 'AppyTappy looks for the picture in the search area each time, and clicks only when it finds it.'}
          </span>
        </div>
        {mode === 'picture' ? (
          <>
            {drawn === null && initial?.kind === 'picture' ? (
              <div className="field">
                <span>Picture</span>
                <ClickPictureThumb image={initial.image} label="The picture it clicks on now" />
                <span className="note">Draw on a screenshot to replace it.</span>
              </div>
            ) : null}
            <div className="field">
              <span className="name">
                <span id="click-step">Mark</span>
                <Hint topic="Mark" id="hint-click-step">
                  First mark the picture to look for, such as a button. Then mark where on it to click; it starts in the
                  middle.
                </Hint>
              </span>
              <div className="seg" role="group" aria-labelledby="click-step" aria-describedby="hint-click-step">
                <button
                  type="button"
                  aria-pressed={step === 'picture'}
                  onClick={() => {
                    setStep('picture');
                  }}
                >
                  The picture
                </button>
                <button
                  type="button"
                  aria-pressed={step === 'press'}
                  disabled={drawnHere === null}
                  onClick={() => {
                    setStep('press');
                  }}
                >
                  Where to click on it
                </button>
              </div>
            </div>
            {step === 'picture' ? (
              <SelectFields
                tool={tool}
                colorMatch={colorMatch}
                canSelect={shownPicture !== null}
                onTool={setTool}
                onColorMatch={setColorMatch}
              />
            ) : null}
            <div className="field">
              <span className="name">
                <label htmlFor="click-tolerance">How alike it must look</label>
                <Hint topic="How alike it must look" id="hint-click-tolerance">
                  How far each pixel&apos;s color may be from the picture&apos;s, on average, from 0 for exactly alike
                  to {LIMITS.maxTolerance}. Raise it if the button changes a little, such as when it&apos;s highlighted.
                  Lower it if a similar button could be clicked instead.
                </Hint>
              </span>
              <div className="range">
                <input
                  type="range"
                  min={0}
                  max={LIMITS.maxTolerance}
                  value={tolerance}
                  disabled={sending}
                  aria-label="How alike it must look"
                  onChange={(event) => {
                    changed();
                    onTolerance(Number(event.target.value));
                  }}
                />
                <input
                  id="click-tolerance"
                  className="input"
                  type="number"
                  min={0}
                  max={LIMITS.maxTolerance}
                  aria-describedby="hint-click-tolerance"
                  value={tolerance}
                  disabled={sending}
                  onChange={(event) => {
                    const value = Number.parseInt(event.target.value, 10);
                    if (Number.isInteger(value) && value >= 0 && value <= LIMITS.maxTolerance) {
                      changed();
                      onTolerance(value);
                    }
                  }}
                />
              </div>
            </div>
            <div className="field">
              <span className="name">
                <span>Search area</span>
                <Hint topic="Search area" id="hint-click-search">
                  Where in the window the picture is looked for: up to {SEARCH_MARGIN} pixels around it, and at most{' '}
                  {SEARCH_SIDE} by {SEARCH_SIDE}. A small area is quick to search and keeps the click from landing on a
                  look-alike elsewhere.
                </Hint>
              </span>
              <span className="note">
                {area === null
                  ? `Up to ${SEARCH_MARGIN} px around the picture.`
                  : `${area.width} × ${area.height} px around the picture.`}{' '}
                A click needs the window at the size the picture was taken at.
              </span>
            </div>
          </>
        ) : null}
        <div className="field">
          <span className="name">
            <span id="click-button">Click</span>
            <Hint topic="Click" id="hint-click-button">
              Which mouse button is pressed. Double presses the left button twice. A right click usually opens a menu,
              so it leaves the focus where the click put it.
            </Hint>
          </span>
          <div className="seg" role="group" aria-labelledby="click-button" aria-describedby="hint-click-button">
            {BUTTONS.map((option) => (
              <button
                key={option}
                type="button"
                aria-pressed={button === option}
                disabled={sending}
                onClick={() => {
                  tryNext({ kind: 'changed', what: 'press' });
                  onButton(option);
                }}
              >
                {BUTTON_NAMES[option]}
              </button>
            ))}
          </div>
        </div>
        <div className="field">
          <span className="name">
            <span id="click-delivery">How to click</span>
            <Hint topic="How to click" id="hint-click-delivery">
              {DELIVERY_NAMES.cursor} presses the way a real mouse does, and the window comes to the front for a moment.{' '}
              {DELIVERY_NAMES.message} does not move the cursor or bring the window to the front, but some applications
              ignore it.
            </Hint>
          </span>
          <div className="seg" role="group" aria-labelledby="click-delivery" aria-describedby="hint-click-delivery">
            {DELIVERIES.map((option) => (
              <button
                key={option}
                type="button"
                aria-pressed={delivery === option}
                disabled={sending}
                onClick={() => {
                  tryNext({ kind: 'changed', what: 'press' });
                  onDelivery(option);
                }}
              >
                {DELIVERY_NAMES[option]}
                <small>{DELIVERY_WORDS[option]}</small>
              </button>
            ))}
          </div>
          <span className="note">{DELIVERY_NOTES[delivery]}</span>
        </div>
        <div className="field">
          <span className="name">
            <span id="click-try">Try it</span>
            <Hint topic="Try it" id="hint-click-try">
              Find it now shows where the click would land, without clicking. Click it now sends one real click to the
              window, the way the automation would, so you can see whether the application takes it. With several of the
              application&apos;s windows open, it clicks the one the automation would click, which may be another one
              than this screen shows.
            </Hint>
          </span>
          <div className="click-tries" role="group" aria-labelledby="click-try" aria-describedby="hint-click-try">
            <button
              type="button"
              className="btn"
              disabled={busy || !ready || window.kind !== 'picked'}
              title="Looks for the target on the live window now, without clicking."
              onClick={findNow}
            >
              Find it now
            </button>
            <button
              type="button"
              className="btn"
              // While anti-cheat blocks the click, the button stays in the tab order, so its note is read out.
              disabled={blocked === null && (busy || !ready || window.kind !== 'picked')}
              aria-disabled={blocked !== null}
              aria-describedby={blocked === null ? undefined : 'click-blocked'}
              title="Sends one real click to the window now, the way the automation would."
              onClick={clickItNow}
            >
              Click it now
            </button>
          </div>
          {blocked === null ? null : (
            <span className="note" id="click-blocked">
              {blocked}
            </span>
          )}
          <div role="status" ref={answer}>
            {tried.kind === 'finding' ? <p className="note">Looking for it…</p> : null}
            {tried.kind === 'clicking' ? <p className="note">Clicking…</p> : null}
            {tried.kind === 'failed' ? <p className="warn">{tried.message}</p> : null}
            {tried.kind !== 'answered' ? null : tried.answer.kind === 'sent' ? (
              <p className="result">{tried.answer.message}</p>
            ) : (
              <p className="result">
                {tried.answer.at.likeness === null
                  ? `It would click ${tried.answer.at.x} px across and ${tried.answer.at.y} px down the window.`
                  : `Found it, ${tried.answer.at.likeness}% alike. It would click ${tried.answer.at.x} px across and ${tried.answer.at.y} px down the window.`}
              </p>
            )}
          </div>
        </div>
        <span className="warn" role="alert">
          {error ?? ''}
        </span>
        <div className="actions">
          <button type="button" className="btn" disabled={busy} onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="btn primary" disabled={busy || !ready} onClick={use}>
            Use this target
          </button>
        </div>
      </aside>
    </main>
  );
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The automation's name, for the breadcrumb. */
  readonly automation: string;
  /** The ID of the application whose value the automation watches, or null before it watches one. */
  readonly watched: string | null;
  /** The target the click has, or null before it has one. */
  readonly initial: {
    readonly app: string;
    readonly button: ClickButtonDef;
    readonly delivery: ClickDeliveryDef;
    readonly returnFocus: boolean;
    readonly at: ClickAtDef | null;
  };
  readonly onCancel: () => void;
  readonly onUse: (target: ClickTargetValue) => void;
}

/** Setting up where a click lands: on a spot, or on a picture wherever it shows near where it was. */
export function ClickTarget({ apps, automation, watched, initial, onCancel, onUse }: Props) {
  const choices = clickApps(apps);
  const [app, setApp] = useState(() => (initial.app === '' ? (choices[0]?.id ?? '') : initial.app));
  const [button, setButton] = useState<ClickButtonDef>(initial.button);
  const [delivery, setDelivery] = useState<ClickDeliveryDef>(initial.delivery);
  const [tolerance, setTolerance] = useState(() =>
    initial.at?.kind === 'picture' ? initial.at.tolerance : DEFAULT_TOLERANCE,
  );
  const chosen = choices.find((choice) => choice.id === app) ?? null;
  // Each look again opens a new stage, which picks the window afresh and forgets the screenshots.
  const [round, setRound] = useState(0);

  return (
    <TargetStage
      // A new application, or looking again, picks a new window, which forgets the screenshots.
      key={`${app}:${round}`}
      apps={apps}
      app={app}
      appName={chosen?.name ?? app}
      automation={automation}
      initial={initial.app === app ? initial.at : null}
      button={button}
      delivery={delivery}
      returnFocus={returnsFocus(button, initial)}
      watched={watched}
      tolerance={tolerance}
      onButton={setButton}
      onDelivery={setDelivery}
      onTolerance={setTolerance}
      onCancel={onCancel}
      onUse={onUse}
      onLookAgain={() => {
        setRound((count) => count + 1);
      }}
      onApp={setApp}
    />
  );
}
