import { describe, expect, test } from 'bun:test';

import type { WindowDef, WindowFit } from '../src/lib/bindings.ts';
import {
  exactWindow,
  fallbackUseful,
  fitCount,
  fitLabel,
  matchSummary,
  sentence,
  required,
  withMustMatch,
} from '../src/lib/matching.ts';

const example = exactWindow({ title: 'Example App - Save 3', class: 'ExampleWindow', executable: 'Example.exe' });

describe('a new match', () => {
  test('requires only the executable and has no fallback', () => {
    expect(required(example)).toEqual(['executable']);
    expect(example.fallback).toBe('none');
    expect(example.title).toBe('Example App - Save 3');
  });
});

describe('must match', () => {
  test('ticks and unticks the title and class, and always keeps the executable', () => {
    const both = withMustMatch(example, 'class', true);
    expect(required(both)).toEqual(['class', 'executable']);
    expect(required(withMustMatch(both, 'class', false))).toEqual(['executable']);
  });

  test('drops the fallback once only the executable is required', () => {
    const window: WindowDef = { ...withMustMatch(example, 'title', true), fallback: 'executable' };
    expect(withMustMatch(window, 'title', false).fallback).toBe('none');
  });

  test('keeps the fallback while the title or class is required', () => {
    const window: WindowDef = {
      ...withMustMatch(withMustMatch(example, 'title', true), 'class', true),
      fallback: 'executable',
    };
    expect(withMustMatch(window, 'title', false).fallback).toBe('executable');
  });
});

describe('fallbacks', () => {
  test('are hidden when only the executable is required, since every window of it qualifies', () => {
    expect(fallbackUseful(example)).toBe(false);
    expect(fallbackUseful(withMustMatch(example, 'class', true))).toBe(true);
  });
});

describe('the summary', () => {
  test('says what must match, what is preferred and the fallback', () => {
    const window: WindowDef = { ...withMustMatch(example, 'title', true), fallback: 'executable' };
    expect(matchSummary(window)).toEqual({
      must: 'Must match title Example App - Save 3, executable Example.exe',
      then: 'Prefers class ExampleWindow; otherwise any window of that executable',
    });
  });

  test('has no second line when everything must match and nothing is the fallback', () => {
    const all = withMustMatch(withMustMatch(example, 'title', true), 'class', true);
    expect(matchSummary(all).then).toBeNull();
  });
});

describe('the window list', () => {
  const fits: readonly WindowFit[] = [
    { handle: 3, fallback: false, best: true, tied: true },
    { handle: 5, fallback: false, best: false, tied: false },
  ];

  test('marks the best match, the other matches and windows the match does not take', () => {
    expect(fitLabel(fits, 3)?.text).toBe('best match');
    expect(fitLabel(fits, 5)?.text).toBe('matches');
    expect(fitLabel(fits, 9)).toBeNull();
    expect(fitLabel(null, 3)).toBeNull();
  });

  test('marks fallback windows', () => {
    const fallbacks: readonly WindowFit[] = [
      { handle: 3, fallback: true, best: true, tied: true },
      { handle: 5, fallback: true, best: false, tied: false },
    ];
    expect(fitLabel(fallbacks, 3)?.text).toBe('best match, fallback');
    expect(fitLabel(fallbacks, 5)?.text).toBe('fallback');
  });

  test('counts what qualifies and says when the fallback is used', () => {
    expect(fitCount(null)).toBe('Looking for matching windows…');
    expect(fitCount([])).toBe('No open window qualifies, so AppyTappy treats the application as closed.');
    expect(fitCount([{ handle: 3, fallback: true, best: true, tied: true }])).toBe(
      'No open window qualifies, so AppyTappy uses the fallback: 1 window.',
    );
    expect(fitCount(fits.slice(0, 1))).toBe('1 open window qualifies.');
    expect(fitCount(fits)).toBe('2 open windows qualify. AppyTappy uses the one that matches the most fields.');
  });
});

describe('sentence', () => {
  test('ends text in a full stop once', () => {
    expect(sentence('no answer ')).toBe('no answer.');
    expect(sentence('Done!')).toBe('Done!');
  });
});
