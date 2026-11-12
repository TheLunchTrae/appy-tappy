// The matching pixels preview: which pixels of the screenshot shown a color setting takes, as the setup app's own
// readers decide, tinted over the screenshot while the user picks the color and moves its tolerance.

import type { Layer, MatchRule, MatchView, SelectStep } from './bindings.ts';
import type { Draft } from './check.ts';
import type { Color } from './pixels.ts';
import { type Rect, type Size, sameRect } from './region.ts';
import type { Selection } from './selection.ts';

/** The color settings that decide pixel by pixel: a reference's characters, a bar's two colors, a color check's. */
export type PreviewTarget = MatchRule['kind'];

/** What the preview asks the setup app: the pixels of `area` on a screenshot that `rule` takes. */
export interface PreviewRequest {
  readonly screenshot: number;
  /** In the screenshot's pixels. */
  readonly area: Rect;
  readonly rule: MatchRule;
}

/** What the preview can ask now, or why it has nothing to show. */
export type Plan =
  | { readonly kind: 'ask'; readonly request: PreviewRequest }
  | { readonly kind: 'blocked'; readonly why: string };

/** The parts of a draft the preview reads. */
export type PreviewDraft = Pick<Draft, 'ink' | 'tolerance' | 'filled' | 'empty' | 'color' | 'region'>;

const DRAW_FIRST = 'Draw the region on the screenshot to see its matching pixels.';

/**
 * What to ask for `target` of `draft` on the screenshot shown, `selected`, whose size is `size` once its pixels
 * arrived. A reference's characters are looked for on the whole screenshot, to show what else has their color; a
 * bar's and a color check's pixels in the region, which is all the check reads.
 */
export function plan(target: PreviewTarget, draft: PreviewDraft, selected: number | null, size: Size | null): Plan {
  if (selected === null) {
    return { kind: 'blocked', why: 'Matching pixels show on a screenshot. Select one, or take one.' };
  }
  const blocked = (why: string): Plan => ({ kind: 'blocked', why });
  const ask = (area: Rect, rule: MatchRule): Plan => ({ kind: 'ask', request: { screenshot: selected, area, rule } });
  switch (target) {
    case 'ink':
      if (draft.ink === null) {
        return blocked("Pick the characters' color to see its matching pixels.");
      }
      if (size === null) {
        return blocked('Waiting for the screenshot…');
      }
      return ask(
        { x: 0, y: 0, width: size.width, height: size.height },
        { kind: 'ink', ink: draft.ink, tolerance: draft.tolerance },
      );
    case 'barFill':
      if (draft.filled === null || draft.empty === null) {
        return blocked('Pick the filled and the empty color to see their matching pixels.');
      }
      if (draft.region === null) {
        return blocked(DRAW_FIRST);
      }
      return ask(draft.region, { kind: 'barFill', filled: draft.filled, empty: draft.empty });
    case 'color':
      if (draft.color === null) {
        return blocked('Pick the color to look for to see its matching pixels.');
      }
      if (draft.region === null) {
        return blocked(DRAW_FIRST);
      }
      return ask(draft.region, { kind: 'color', color: draft.color, tolerance: draft.tolerance });
  }
}

/** An answer and the request it answers. */
export interface Answered {
  readonly request: PreviewRequest;
  readonly view: MatchView;
}

/** A request to send, and the number its answer must carry. */
interface Sent {
  readonly id: number;
  readonly request: PreviewRequest;
}

/**
 * The preview's requests and answers. It is idle while `flight` is null, in flight while a request is out, and in
 * flight with one queued while `queued` also holds the newest request wanted since.
 */
export interface Preview {
  /** The number of the last request sent, so each request gets its own. */
  readonly sent: number;
  /** The request whose answer is awaited, or null while none is. */
  readonly flight: Sent | null;
  /** The newest request wanted since the one in flight went out, which goes out when that one is answered. */
  readonly queued: PreviewRequest | null;
  /** The newest answer, kept while the next one is awaited. */
  readonly shown: Answered | null;
  /** Why the newest request failed. */
  readonly error: string | null;
}

/** The preview before anything is asked, and once it is turned off. */
export const NO_PREVIEW: Preview = { sent: 0, flight: null, queued: null, shown: null, error: null };

export type PreviewEvent =
  /** The settings ask for `request` now. */
  | { readonly kind: 'wanted'; readonly request: PreviewRequest }
  | { readonly kind: 'answered'; readonly id: number; readonly view: MatchView }
  | { readonly kind: 'failed'; readonly id: number; readonly message: string }
  /** The preview was turned off, or has nothing to ask. */
  | { readonly kind: 'cleared' };

/** The preview after an event, and the request to send now, if any. */
export interface Stepped {
  readonly preview: Preview;
  readonly send: Sent | null;
}

function sameRequest(a: PreviewRequest, b: PreviewRequest): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** `preview` with `request` sent under the next number. */
function sending(preview: Preview, request: PreviewRequest): Stepped {
  const send = { id: preview.sent + 1, request };
  return { preview: { ...preview, sent: send.id, flight: send, queued: null, error: null }, send };
}

/** `preview` once the request in flight is settled: the queued request goes out, or the preview is idle. */
function settled(preview: Preview): Stepped {
  return preview.queued === null
    ? { preview: { ...preview, flight: null }, send: null }
    : sending(preview, preview.queued);
}

/**
 * The preview after `event`, and the request to send. At most one request is in flight. A request wanted meanwhile
 * waits, and a newer one takes its place, so a burst of changes sends the first and the last. Only the answer or
 * failure of the request in flight counts: one that carries another number, such as one sent before the preview was
 * cleared, changes nothing and sends nothing.
 */
export function step(preview: Preview, event: PreviewEvent): Stepped {
  switch (event.kind) {
    case 'wanted':
      if (preview.flight === null) {
        return sending(preview, event.request);
      }
      // The request in flight already asks this, so nothing needs to follow it.
      return {
        preview: { ...preview, queued: sameRequest(preview.flight.request, event.request) ? null : event.request },
        send: null,
      };
    case 'answered':
      if (event.id !== preview.flight?.id) {
        return { preview, send: null };
      }
      return settled({ ...preview, shown: { request: preview.flight.request, view: event.view }, error: null });
    case 'failed':
      if (event.id !== preview.flight?.id) {
        return { preview, send: null };
      }
      return settled({ ...preview, shown: null, error: event.message });
    case 'cleared':
      return { preview: { ...NO_PREVIEW, sent: preview.sent }, send: null };
  }
}

/**
 * Runs a preview: sends each request {@link step} orders with `ask`, and steps with its answer or failure under the
 * number the request was sent with. `show` hears the preview after each step. The function it returns takes the
 * request the settings want now, or null to clear the preview.
 */
export function previewDriver(
  ask: (request: PreviewRequest) => Promise<MatchView>,
  show: (preview: Preview) => void,
  describe: (error: unknown) => string,
): (request: PreviewRequest | null) => void {
  let preview = NO_PREVIEW;
  const apply = (event: PreviewEvent): void => {
    const stepped = step(preview, event);
    ({ preview } = stepped);
    show(preview);
    const { send } = stepped;
    if (send === null) {
      return;
    }
    ask(send.request).then(
      (view) => {
        apply({ kind: 'answered', id: send.id, view });
      },
      (error: unknown) => {
        apply({ kind: 'failed', id: send.id, message: describe(error) });
      },
    );
  };
  return (request) => {
    apply(request === null ? { kind: 'cleared' } : { kind: 'wanted', request });
  };
}

/**
 * The answer to tint over the screenshot while `request` is the one to show, or null. An answer for the same
 * rectangle of the same screenshot stays while a new color or tolerance is asked about, so the tint does not flicker
 * under a dragged slider; one for another screenshot or rectangle would tint the wrong pixels, and goes at once.
 */
export function tinted(preview: Preview, request: PreviewRequest | null): Answered | null {
  const { shown } = preview;
  if (request === null || shown === null) {
    return null;
  }
  const same =
    shown.request.screenshot === request.screenshot &&
    shown.request.rule.kind === request.rule.kind &&
    sameRect(shown.request.area, request.area);
  return same ? shown : null;
}

/**
 * Whether the answer for `request` is still awaited, so what shows is for earlier settings: nothing answers it yet,
 * and it has not failed.
 */
export function awaited(preview: Preview, request: PreviewRequest | null): boolean {
  if (request === null) {
    return false;
  }
  const { flight, shown, error } = preview;
  return flight !== null || (error === null && (shown === null || !sameRequest(shown.request, request)));
}

/** The layers of `view`, in the order {@link tintsFor} gives their tints. */
export function layersOf(view: MatchView): readonly Layer[] {
  switch (view.kind) {
    case 'ink':
      return [view.ink];
    case 'barFill':
      return [view.filled, view.empty];
    case 'color':
      return [view.near];
  }
}

/** How a layer's pixels are tinted. */
export interface Tint {
  readonly color: Color;
  /** Whether only diagonal stripes are tinted, to tell a second layer from the first without its color. */
  readonly striped: boolean;
}

const MAGENTA: Color = { red: 255, green: 0, blue: 255 };
/** The tint of a selection's own pixels, which have no one color to stand out against. */
export const SELECTED_TINT: Tint = { color: MAGENTA, striped: false };
const GREEN: Color = { red: 0, green: 255, blue: 0 };

function apart(a: Color, b: Color): number {
  return (a.red - b.red) ** 2 + (a.green - b.green) ** 2 + (a.blue - b.blue) ** 2;
}

/** Magenta and green, the one that stands out more against `picked` first. */
function contrasting(picked: Color): readonly [Color, Color] {
  return apart(picked, MAGENTA) >= apart(picked, GREEN) ? [MAGENTA, GREEN] : [GREEN, MAGENTA];
}

/**
 * The tint of each layer of `rule`'s answer: whichever of magenta and green stands out more against the picked
 * color. A bar's empty pixels take the other one, in stripes.
 */
export function tintsFor(rule: MatchRule): readonly Tint[] {
  switch (rule.kind) {
    case 'ink':
      return [{ color: contrasting(rule.ink)[0], striped: false }];
    case 'color':
      return [{ color: contrasting(rule.color)[0], striped: false }];
    case 'barFill': {
      const [first, second] = contrasting(rule.filled);
      return [
        { color: first, striped: false },
        { color: second, striped: true },
      ];
    }
  }
}

/** How opaque a tint is, out of 255: enough to stand out, with the screenshot still showing through. */
const TINT_ALPHA = 150;
/** How opaque a stripe is; the pixels between stripes stay clear. */
const STRIPE_ALPHA = 215;
/** A stripe covers two of every four diagonals. */
const STRIPE_PERIOD = 4;

/** `color` at `alpha` as the 32-bit number that sets one pixel of RGBA bytes through a `Uint32Array`. */
function packed(color: Color, alpha: number): number {
  return new Uint32Array(new Uint8Array([color.red, color.green, color.blue, alpha]).buffer)[0] ?? 0;
}

/** The runs of one kind of pixel, or null when there are too many to draw. */
type Runs = Pick<Layer, 'spans'>;

/** What to tint over the screenshot: the runs of each layer within `area`, and each layer's tint. */
export interface Painted {
  /** In the screenshot's pixels. */
  readonly area: Rect;
  readonly layers: readonly Runs[];
  readonly tints: readonly Tint[];
}

/** An answer as the overlay tints it. */
export function paintedOf(answered: Answered): Painted {
  return { area: answered.request.area, layers: layersOf(answered.view), tints: tintsFor(answered.request.rule) };
}

/**
 * The RGBA pixels that tint `painted`'s layers over its rectangle, each with its entry in `tints`: clear where no
 * layer has a pixel. A layer with too many runs to draw is left clear.
 */
export function paint({ area, layers, tints }: Painted): Uint8ClampedArray<ArrayBuffer> {
  const { width } = area;
  const pixels = new Uint32Array(width * area.height);
  layers.forEach((layer, index) => {
    const tint = tints[index];
    if (tint === undefined || layer.spans === null) {
      return;
    }
    if (!tint.striped) {
      const solid = packed(tint.color, TINT_ALPHA);
      for (const [start, end] of layer.spans) {
        pixels.fill(solid, start, end);
      }
      return;
    }
    const stripe = packed(tint.color, STRIPE_ALPHA);
    for (const [start, end] of layer.spans) {
      for (let at = start; at < end && at < pixels.length; at += 1) {
        if ((Math.floor(at / width) + (at % width)) % STRIPE_PERIOD < STRIPE_PERIOD / 2) {
          pixels[at] = stripe;
        }
      }
    }
  });
  return new Uint8ClampedArray(pixels.buffer);
}

/**
 * How many of a layer's pixels lie in `region`, when its `spans` count within `area`; both rectangles are in the
 * screenshot's pixels.
 */
export function countIn(spans: readonly (readonly [number, number])[], area: Rect, region: Rect): number {
  const left = Math.max(region.x - area.x, 0);
  const right = Math.min(region.x + region.width - area.x, area.width);
  const top = Math.max(region.y - area.y, 0);
  const bottom = Math.min(region.y + region.height - area.y, area.height);
  if (left >= right || top >= bottom) {
    return 0;
  }
  let count = 0;
  for (const [start, end] of spans) {
    const first = Math.max(Math.floor(start / area.width), top);
    const last = Math.min(Math.floor((end - 1) / area.width), bottom - 1);
    for (let row = first; row <= last; row += 1) {
      const from = Math.max(start - row * area.width, left);
      const to = Math.min(end - row * area.width, right);
      count += Math.max(to - from, 0);
    }
  }
  return count;
}

function pixelsText(count: number): string {
  return count === 1 ? '1 pixel' : `${count.toLocaleString('en-US')} pixels`;
}

/** `count` of `total` as a percentage that reads 0% and 100% only when it is none and all of them. */
export function share(count: number, total: number): string {
  if (count <= 0 || total <= 0) {
    return '0%';
  }
  if (count >= total) {
    return '100%';
  }
  const percent = (count / total) * 100;
  if (percent < 1) {
    return 'under 1%';
  }
  return percent > 99 ? 'over 99%' : `${Math.round(percent)}%`;
}

const TOO_MANY = 'Too many separate areas match to tint them all, so none is tinted.';

function area(rect: Rect): number {
  return rect.width * rect.height;
}

/** Whether `inner` lies inside `outer`. */
function holds(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}

/** How a count of a region's pixels reads. */
function ofRegion(count: number, total: number): string {
  return `${count.toLocaleString('en-US')} of ${pixelsText(total)} in the region (${share(count, total)})`;
}

/**
 * What a reference's tint shows, in words: how many pixels count as its characters in `region`, the draft's region,
 * and on the rest of the screenshot. A region that reaches past the screenshot shown is left out.
 */
export function inkLines(layer: Layer, asked: Rect, region: Rect | null): readonly string[] {
  const total = area(asked);
  if (layer.spans === null) {
    return [
      `${TOO_MANY} ${layer.count.toLocaleString('en-US')} of ${pixelsText(total)} on the screenshot (${share(layer.count, total)}) count as characters. Lower the tolerance.`,
    ];
  }
  if (layer.count === 0) {
    return ['No pixel on the screenshot counts as characters. Raise the tolerance, or pick the color again.'];
  }
  if (region === null || !holds(asked, region)) {
    return [
      `${layer.count.toLocaleString('en-US')} of ${pixelsText(total)} on the screenshot (${share(layer.count, total)}) count as characters.`,
    ];
  }
  const inside = countIn(layer.spans, asked, region);
  const elsewhere = `${pixelsText(layer.count - inside)} elsewhere on the screenshot`;
  const within = area(region);
  if (inside === 0) {
    return [`No pixel in the region counts as characters; ${elsewhere} ${layer.count === 1 ? 'does' : 'do'}.`];
  }
  const counted = `${ofRegion(inside, within)} count as characters`;
  const rest = layer.count === inside ? 'none elsewhere on the screenshot' : elsewhere;
  return inside === within
    ? [`Every pixel in the region counts as characters, and ${rest}. Lower the tolerance until only the characters do.`]
    : [`${counted}, and ${rest}.`];
}

/** One line of a bar's legend: a kind of pixel, how many there are, and its tint, or null for pixels left clear. */
export interface LegendEntry {
  readonly name: string;
  readonly count: string;
  readonly tint: Tint | null;
}

/** A bar's legend: its filled, empty, and other pixels in the region. */
export function barLegend(
  filled: Layer,
  empty: Layer,
  asked: Rect,
  tints: readonly Tint[],
): { readonly entries: readonly LegendEntry[]; readonly note: string | null } {
  const total = area(asked);
  const entry = (name: string, count: number, tint: Tint | null): LegendEntry => ({
    name,
    count: `${count.toLocaleString('en-US')} (${share(count, total)})`,
    tint,
  });
  const neither = total - filled.count - empty.count;
  const entries = [
    entry('Filled', filled.count, tints[0] ?? null),
    entry('Empty', empty.count, tints[1] ?? null),
    entry('Neither, left out', neither, null),
  ];
  if (filled.count + empty.count === 0) {
    return { entries, note: 'No pixel in the region is near the filled or the empty color. Pick the colors again.' };
  }
  return { entries, note: filled.spans === null || empty.spans === null ? TOO_MANY : null };
}

/** What a color check's tint shows, in words: how many of the region's pixels are within the tolerance alone. */
export function nearLine(layer: Layer, asked: Rect): string {
  const total = area(asked);
  if (layer.count === 0) {
    return 'No pixel in the region is within the tolerance on its own.';
  }
  if (layer.count === total) {
    return 'Every pixel in the region is within the tolerance on its own.';
  }
  const counted = `${ofRegion(layer.count, total)} ${layer.count === 1 ? 'is' : 'are'} within the tolerance on ${layer.count === 1 ? 'its' : 'their'} own.`;
  return layer.spans === null ? `${TOO_MANY} ${counted}` : counted;
}

/** The selection a state's steps make on its screenshot, as much of it as the capture screen has. */
interface Selecting {
  readonly screenshot: number;
  readonly steps: readonly SelectStep[];
}

/** What **Show selected pixels** shows of the selection on the screenshot shown. */
export interface SelectedPixels {
  /** What is selected, in words, or why nothing shows. */
  readonly line: string;
  /** The selection's own pixels to tint, or null. */
  readonly painted: Painted | null;
  /** Whether the selection for the color match now is still awaited, so what shows is for an earlier one. */
  readonly updating: boolean;
}

/**
 * What to show of the selection `states` has on the screenshot shown, `selected`, at `colorMatch`. `selections`
 * holds the selection last made on each screenshot. One made for the same steps at an earlier color match stays
 * until the new one is made, so the tint does not flicker under the slider.
 */
export function selectedPixels(
  states: readonly Selecting[],
  selections: ReadonlyMap<number, Selection>,
  selected: number | null,
  colorMatch: number,
): SelectedPixels {
  const nothing = (line: string, updating = false): SelectedPixels => ({ line, painted: null, updating });
  if (selected === null) {
    return nothing('Selected pixels show on a screenshot. Select the one the thing was selected on.');
  }
  const state = states.find((each) => each.screenshot === selected);
  if (state === undefined || state.steps.length === 0) {
    return nothing('Nothing is selected on this screenshot yet. Select the thing on it with Snap or Click.');
  }
  const made = selections.get(selected);
  if (made === undefined || made.steps !== state.steps) {
    return nothing('Selecting…', true);
  }
  const updating = made.colorMatch !== colorMatch;
  const { view } = made;
  if (view === null) {
    return nothing('Nothing is selected at this color match. Change it, or select the thing again.', updating);
  }
  const count = view.spans.reduce((sum, [start, end]) => sum + end - start, 0);
  const box = area(view.region);
  return {
    line: `${count.toLocaleString('en-US')} of the ${pixelsText(box)} in the box around the selection (${share(count, box)}) are selected.`,
    painted: { area: view.region, layers: [view], tints: [SELECTED_TINT] },
    updating,
  };
}
