// Trying a click target: what Find it now and Click it now are doing, and what each answered.

import type { FoundClick } from './bindings.ts';

/** What a try answered: where the click would land, or that a click was sent. */
export type TryAnswer =
  | { readonly kind: 'found'; readonly at: FoundClick }
  | {
      readonly kind: 'sent';
      readonly message: string;
      /** Whether the click went to another window than the one the screen shows. */
      readonly elsewhere: boolean;
    };

/**
 * Trying a click target. A try under way has a number, so an answer that comes back after the target changed, or after
 * a later try started, is told apart and dropped. A find can be dropped that way. A click cannot: it is a real click
 * once it is asked for, so the target stays as it is until its answer shows.
 */
export type Try =
  | { readonly kind: 'idle' }
  | { readonly kind: 'finding'; readonly attempt: number }
  | { readonly kind: 'clicking'; readonly attempt: number }
  | { readonly kind: 'answered'; readonly answer: TryAnswer }
  | { readonly kind: 'failed'; readonly message: string };

/** What moves a try on. */
export type TryEvent =
  /** Find it now or Click it now started, as try number `attempt`. */
  | { readonly kind: 'find' | 'click'; readonly attempt: number }
  | { readonly kind: 'answered'; readonly attempt: number; readonly answer: TryAnswer }
  | { readonly kind: 'failed'; readonly attempt: number; readonly message: string }
  /**
   * The user changed the target: `place` for where it clicks, which no earlier answer speaks for, or `press` for the
   * button or the way it clicks, which leaves where it would land as it was. The screen lets nothing change while a
   * click is being sent, and a change that comes then all the same does not drop the click's answer.
   */
  | { readonly kind: 'changed'; readonly what: 'place' | 'press' };

export const IDLE: Try = { kind: 'idle' };

/** The try after `event`. */
export function tryStep(state: Try, event: TryEvent): Try {
  switch (event.kind) {
    case 'find':
      return { kind: 'finding', attempt: event.attempt };
    case 'click':
      return { kind: 'clicking', attempt: event.attempt };
    case 'changed':
      if (state.kind === 'clicking') {
        return state;
      }
      return event.what === 'press' && state.kind === 'answered' && state.answer.kind === 'found' ? state : IDLE;
    case 'answered':
    case 'failed':
      // An answer to a try that a change or a later try replaced says nothing about the target as it is set up now.
      if ((state.kind !== 'finding' && state.kind !== 'clicking') || state.attempt !== event.attempt) {
        return state;
      }
      return event.kind === 'answered'
        ? { kind: 'answered', answer: event.answer }
        : { kind: 'failed', message: event.message };
  }
}
