import { type PointerEvent, type ReactNode, useCallback, useEffect, useRef, useState } from 'react';

import {
  addClickPicture,
  clickPicture,
  errorText,
  findClick,
  pickWindow,
  readWindow,
  select,
  stopCapture,
} from '../lib/api.ts';
import type { AppSummary, ClickAtDef, ClickButtonDef, FoundClick, SelectionSpec } from '../lib/bindings.ts';
import {
  BUTTONS,
  BUTTON_NAMES,
  DEFAULT_TOLERANCE,
  MAX_SEARCH_PIXELS,
  MAX_TOLERANCE,
  clickApps,
  middle,
  placeSpot,
  searchArea,
  spotAt,
} from '../lib/click.ts';
import type { Picture } from '../lib/pixels.ts';
import { type Point, type Rect, type Size, contains, fit, toPicture } from '../lib/region.ts';
import { background } from '../lib/repeat.ts';
import { DEFAULT_COLOR_MATCH, type SelectTool } from '../lib/selection.ts';
import { useSize } from '../lib/size.ts';
import { useScreenshotShortcut } from '../lib/useScreenshotShortcut.ts';
import { useScreenshots } from '../lib/useScreenshots.ts';
import { LiveCanvas, PictureCanvas } from './canvas.tsx';
import { SelectFields } from './CheckFields.tsx';
import { Hint } from './Hint.tsx';
import { RegionSurface } from './RegionSurface.tsx';

/** The widest live frame asked for, which keeps a large window's preview cheap. */
const LIVE_MAX_WIDTH = 1600;

/** The widest a click picture's thumbnail is asked for. */
const THUMB_WIDTH = 220;

/** A click's target as the automation editor keeps it. */
interface ClickTargetValue {
  readonly app: string;
  readonly button: ClickButtonDef;
  readonly at: ClickAtDef;
}

/** A click picture shown small, such as beside a click in the automation editor. */
export function ClickPictureThumb({ image, label }: { readonly image: string; readonly label: string }) {
  const [picture, setPicture] = useState<Picture | null>(null);
  useEffect(() => {
    let current = true;
    background(
      clickPicture(image, THUMB_WIDTH)
        .then((loaded) => {
          if (current) {
            setPicture(loaded);
          }
        })
        .catch(() => {
          // The summary beside it still says what the click does.
        }),
    );
    return () => {
      current = false;
    };
  }, [image]);
  return (
    <span className="click-thumb">{picture === null ? null : <PictureCanvas picture={picture} label={label} />}</span>
  );
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
  | { readonly kind: 'picked' }
  | { readonly kind: 'missing'; readonly message: string };

/** What Find it now found: where the click lands, or why it lands nowhere. */
type Found =
  | { readonly kind: 'found'; readonly at: FoundClick }
  | { readonly kind: 'missed'; readonly message: string };

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
  readonly tolerance: number;
  /** The application field, disabled while a picture is kept or looked for. */
  readonly appField: (busy: boolean) => ReactNode;
  /** Opens the screen again on the application, to find its window anew. */
  readonly onLookAgain: () => void;
  readonly onButton: (button: ClickButtonDef) => void;
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
  tolerance,
  appField,
  onLookAgain,
  onButton,
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
  const [found, setFound] = useState<Found | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The picture kept for a drawn region, so finding and using it keep it once.
  const kept = useRef<{ readonly key: string; readonly at: ClickAtDef } | null>(null);
  // Whether the screen still shows this stage, so a target finished after it closed is not used.
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const screenshots = useScreenshots();
  const { shots, full, selected, setSelected, position } = screenshots;
  const shortcut = useScreenshotShortcut(screenshots.take);

  const file = apps.find((candidate) => candidate.id === app)?.file ?? null;
  // The application's file as the lists last read it, which the window is picked by.
  const latestFile = useRef(file);
  useEffect(() => {
    latestFile.current = file;
  });

  // Picks the window the runtime clicks in, which it finds by the application's rules without the fallback.
  useEffect(() => {
    let current = true;
    // A function, because the check after an await must see a cleanup made meanwhile.
    const isCurrent = (): boolean => current;
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
            if (isCurrent()) {
              setWindow({
                kind: 'missing',
                message: `No open window matches ${file.name}'s window rules. Open it, then look again.`,
              });
            }
            return;
          }
          // A screen closed meanwhile has stopped capture, which a pick now would start again.
          if (!isCurrent()) {
            return;
          }
          await pickWindow(read);
          if (isCurrent()) {
            setWindow({ kind: 'picked' });
          }
        } catch (caught: unknown) {
          if (isCurrent()) {
            setWindow({ kind: 'missing', message: errorText(caught) });
          }
        }
      })(),
    );
    return () => {
      current = false;
    };
    // The window rules are read when the stage opens, and looking again opens a new one, not each time the lists
    // are read again.
  }, []);

  const view = useRef<HTMLDivElement>(null);
  const space = useSize(view);
  const [liveSize, setLiveSize] = useState<Size | null>(null);
  const [liveError, setLiveError] = useState<string | null>(null);
  const onFrame = useCallback((width: number, height: number): void => {
    setLiveError(null);
    setLiveSize((current) => (current?.width === width && current.height === height ? current : { width, height }));
  }, []);
  const onLiveError = useCallback((caught: unknown): void => {
    setLiveError(errorText(caught));
  }, []);

  const shownPicture = selected === null ? null : (full.get(selected) ?? null);
  // The window's own size: a screenshot's, since the live view may be scaled down.
  const newest = shots.at(-1);
  const native = shownPicture ?? (newest === undefined ? null : (full.get(newest.index) ?? null));
  const nativeSize: Size | null = native === null ? null : { width: native.width, height: native.height };
  const layoutSize: Size | null = selected === null ? liveSize : shownPicture;
  const shown = layoutSize === null ? { width: 0, height: 0 } : fit(layoutSize, space);
  const frameStyle = {
    left: (space.width - shown.width) / 2,
    top: (space.height - shown.height) / 2,
    width: shown.width,
    height: shown.height,
  };
  const scale = nativeSize === null || nativeSize.width === 0 ? 0 : shown.width / nativeSize.width;
  const marker = (point: Point) => ({ left: (point.x + 0.5) * scale, top: (point.y + 0.5) * scale });
  const box = (rect: Rect) => ({
    left: rect.x * scale,
    top: rect.y * scale,
    width: rect.width * scale,
    height: rect.height * scale,
  });

  const drawnHere = drawn?.screenshot === selected ? drawn : null;
  const pictureSize = shownPicture === null ? null : { width: shownPicture.width, height: shownPicture.height };
  const area = drawnHere === null || pictureSize === null ? null : searchArea(drawnHere.region, pictureSize);

  const changed = (): void => {
    setFound(null);
    setError(null);
  };

  const setRegion = (region: Rect, selection: SelectionSpec | null): void => {
    if (selected === null) {
      return;
    }
    changed();
    setDrawn({ screenshot: selected, region, selection, press: middle(region) });
  };

  const selectWith = (steps: SelectionSpec['steps']): void => {
    if (selected === null) {
      return;
    }
    const on = selected;
    const selection: SelectionSpec = { tolerance: colorMatch, steps };
    background(
      select(on, selection)
        .then((view_) => {
          if (view_ === null) {
            setError('Nothing was selected there. Try a bigger box or a higher color match.');
            return;
          }
          setRegion(view_.region, selection);
          setStep('press');
        })
        .catch((caught: unknown) => {
          setError(errorText(caught));
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
    return {
      x: Math.min(Math.floor(point.x), pictureSize.width - 1),
      y: Math.min(Math.floor(point.y), pictureSize.height - 1),
    };
  };
  const onPoint = (event: PointerEvent<HTMLDivElement>): void => {
    const point = pointAt(event);
    if (point === null || pictureSize === null) {
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

  /** The target's place as set up now, keeping a newly drawn picture first; null with nothing set up. */
  const target = async (): Promise<ClickAtDef | null> => {
    if (mode === 'spot') {
      return spot;
    }
    if (drawn === null) {
      return initial?.kind === 'picture' ? { ...initial, tolerance } : null;
    }
    const picture = full.get(drawn.screenshot);
    if (picture === undefined) {
      return null;
    }
    const size = { width: picture.width, height: picture.height };
    const search = searchArea(drawn.region, size);
    if (search === null) {
      throw new Error(
        `The picture is too big to look for. Draw it smaller: at most ${MAX_SEARCH_PIXELS.toLocaleString()} pixels.`,
      );
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

  const run = (task: (at: ClickAtDef) => Promise<void>): void => {
    setBusy(true);
    setError(null);
    background(
      (async (): Promise<void> => {
        try {
          const at = await target();
          if (at === null) {
            setError(
              mode === 'spot' ? 'Click where the click should land first.' : 'Draw the picture to click on first.',
            );
            return;
          }
          await task(at);
        } catch (caught: unknown) {
          setError(caught instanceof Error ? caught.message : errorText(caught));
        } finally {
          setBusy(false);
        }
      })(),
    );
  };

  const findNow = (): void => {
    run(async (at) => {
      try {
        setFound({ kind: 'found', at: await findClick(at) });
      } catch (caught: unknown) {
        setFound({ kind: 'missed', message: errorText(caught) });
      }
      // The result shows on the live window.
      setSelected(null);
    });
  };

  const use = (): void => {
    run(async (at) => {
      if (mounted.current) {
        onUse({ app, button, at });
      }
      await Promise.resolve();
    });
  };

  const spotHere = spot?.kind === 'spot' && pictureSize !== null ? placeSpot(spot, pictureSize) : null;
  const ready = mode === 'spot' ? spot !== null : drawn !== null || initial?.kind === 'picture';
  const pointing = shownPicture !== null && (mode === 'spot' || (step === 'press' && drawnHere !== null));

  return (
    <main className="capture">
      <section className="stage" aria-label="Click target">
        <div className="bar">
          <span className="crumb">
            Automations / {automation} / <b>Click target</b>
          </span>
          <span className="pill">
            <span className="dot live" />
            Live · {appName}
          </span>
          {native === null ? null : (
            <span className="pill">
              {native.width} × {native.height}
            </span>
          )}
          <div className="end">
            {window.kind === 'missing' || (window.kind === 'picked' && liveError !== null) ? (
              <button
                type="button"
                className="btn"
                disabled={busy}
                title="Finds the application's window again. Screenshots taken so far are cleared."
                onClick={onLookAgain}
              >
                Look again
              </button>
            ) : null}
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
              disabled={window.kind !== 'picked'}
              onClick={() => {
                background(screenshots.take());
              }}
            >
              Take screenshot<span className="kbd">{shortcut.label}</span>
            </button>
          </div>
        </div>
        <p className="note" role="status">
          {shortcut.choosing ? 'Press the new shortcut, or Esc to keep this one.' : null}
        </p>
        {shortcut.error === null ? null : <p className="warn">{shortcut.error}</p>}
        {screenshots.error === null ? null : <p className="warn">{screenshots.error}</p>}
        <div className="view" ref={view}>
          <div className="frame" style={frameStyle}>
            {window.kind !== 'picked' ? null : selected === null ? (
              <LiveCanvas
                maxWidth={Math.min(LIVE_MAX_WIDTH, Math.max(1, Math.round(space.width * devicePixelRatio)))}
                onFrame={onFrame}
                onError={onLiveError}
              />
            ) : shownPicture === null ? null : (
              <PictureCanvas picture={shownPicture} label={`Screenshot ${position(selected)}`} />
            )}
            {mode === 'picture' && area !== null ? (
              <div className="search-area" style={box(area)}>
                <span className="tag">
                  Search area · {area.width} × {area.height}
                </span>
              </div>
            ) : null}
            {mode === 'picture' && shownPicture !== null && !pointing ? (
              <RegionSurface
                size={shownPicture}
                shown={shown}
                region={drawnHere?.region ?? null}
                label="Picture"
                editable
                picking={false}
                tool={tool}
                onRegion={(region) => {
                  setRegion(region, null);
                }}
                onPick={() => {
                  // Clicks pick no colors.
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
                {mode === 'picture' && drawnHere !== null ? (
                  <div className="marquee" style={box(drawnHere.region)}>
                    <span className="tag">
                      Picture · {drawnHere.region.width} × {drawnHere.region.height}
                    </span>
                  </div>
                ) : null}
              </div>
            ) : null}
            {mode === 'picture' && drawnHere !== null ? (
              <span
                className="click-spot"
                style={marker({ x: drawnHere.region.x + drawnHere.press.x, y: drawnHere.region.y + drawnHere.press.y })}
                aria-hidden="true"
              />
            ) : null}
            {mode === 'spot' && spotHere !== null ? (
              <span className="click-spot" style={marker(spotHere)} aria-hidden="true" />
            ) : null}
            {selected === null && found?.kind === 'found' && nativeSize !== null ? (
              <span className="click-spot found" style={marker(found.at)} aria-hidden="true" />
            ) : null}
          </div>
          <p className="hint">
            {selected === null && liveError !== null
              ? liveError
              : hintText(window, mode, step, selected, shortcut.label, tool)}
          </p>
        </div>
        <div className="strip">
          <button
            type="button"
            className="thumb"
            aria-pressed={selected === null}
            onClick={() => {
              setSelected(null);
            }}
          >
            <span className="shot">
              <span className="dot live" />
            </span>
            Live
          </button>
          {shots.map((shot, index) => (
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
              {index + 1} · {shot.time}
            </button>
          ))}
          <span className="note">Screenshots last until you change the application.</span>
        </div>
      </section>
      <aside className="inspector" aria-label="Click target">
        {appField(busy)}
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
                  to 255. Raise it if the button changes a little, such as when it&apos;s highlighted. Lower it if a
                  similar button could be clicked instead.
                </Hint>
              </span>
              <div className="range">
                <input
                  type="range"
                  min={0}
                  max={MAX_TOLERANCE}
                  value={tolerance}
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
                  max={MAX_TOLERANCE}
                  aria-describedby="hint-click-tolerance"
                  value={tolerance}
                  onChange={(event) => {
                    const value = Number.parseInt(event.target.value, 10);
                    if (Number.isInteger(value) && value >= 0 && value <= MAX_TOLERANCE) {
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
                  Where in the window the picture is looked for: up to 50 pixels around it, and at most 400 by 400. A
                  small area is quick to search and keeps the click from landing on a look-alike elsewhere.
                </Hint>
              </span>
              <span className="note">
                {area === null
                  ? 'Up to 50 px around the picture.'
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
                onClick={() => {
                  onButton(option);
                }}
              >
                {BUTTON_NAMES[option]}
              </button>
            ))}
          </div>
        </div>
        <div role="status">
          {found === null ? null : found.kind === 'found' ? (
            <p className="result">
              {found.at.likeness === null
                ? `It would click ${found.at.x} px across and ${found.at.y} px down the window.`
                : `Found it, ${found.at.likeness}% alike. It would click ${found.at.x} px across and ${found.at.y} px down the window.`}
            </p>
          ) : (
            <p className="warn">{found.message}</p>
          )}
        </div>
        <span className="warn" role="alert">
          {error ?? ''}
        </span>
        <div className="actions">
          <button
            type="button"
            className="btn"
            disabled={busy || !ready || window.kind !== 'picked'}
            title="Looks for the target on the live window now, without clicking."
            onClick={findNow}
          >
            Find it now
          </button>
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
  /** The target the click has, or null before it has one. */
  readonly initial: { readonly app: string; readonly button: ClickButtonDef; readonly at: ClickAtDef | null };
  readonly onCancel: () => void;
  readonly onUse: (target: ClickTargetValue) => void;
}

/** Setting up where a click lands: on a spot, or on a picture wherever it shows near where it was. */
export function ClickTarget({ apps, automation, initial, onCancel, onUse }: Props) {
  const choices = clickApps(apps);
  const [app, setApp] = useState(() => (initial.app === '' ? (choices[0]?.id ?? '') : initial.app));
  const [button, setButton] = useState<ClickButtonDef>(initial.button);
  const [tolerance, setTolerance] = useState(() =>
    initial.at?.kind === 'picture' ? initial.at.tolerance : DEFAULT_TOLERANCE,
  );
  const chosen = choices.find((choice) => choice.id === app) ?? null;
  // Each look again opens a new stage, which picks the window afresh and forgets the screenshots.
  const [round, setRound] = useState(0);

  // Capture stops when the screen closes, so the editor's live reading can watch again.
  useEffect(
    () => () => {
      background(stopCapture());
    },
    [],
  );

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
      tolerance={tolerance}
      onButton={setButton}
      onTolerance={setTolerance}
      onCancel={onCancel}
      onUse={onUse}
      onLookAgain={() => {
        setRound((count) => count + 1);
      }}
      appField={(busy) => (
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
            disabled={busy}
            onChange={(event) => {
              setApp(event.target.value);
            }}
          >
            {app === '' ? (
              <option value="" disabled>
                Pick an application
              </option>
            ) : chosen === null ? (
              <option value={app} disabled>
                {app} (no longer saved)
              </option>
            ) : null}
            {choices.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.name}
              </option>
            ))}
          </select>
        </div>
      )}
    />
  );
}
