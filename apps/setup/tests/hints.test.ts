import { expect, test } from 'bun:test';

import { INK_TOLERANCE_HINT, READING_HINT, SHORTCUT_HINT, TOLERANCE_HINT } from '../src/lib/hints.ts';

// These hints are built from the generated limits and key lists. These tests pin the words the user reads.

test('the screenshot shortcut hint lists the keys a shortcut can be', () => {
  expect(SHORTCUT_HINT).toBe(
    "F1 to F24, Print Screen, and Pause work alone. A letter, a digit, Space, an arrow, Insert, Delete, Home, End, Page Up, or Page Down needs Ctrl, Alt, Shift, or Win with it, so it keeps working everywhere else. Other keys can't be the shortcut. Change shortcut on the screenshot page changes the same setting.",
  );
});

test('the range hints name the ranges the save takes', () => {
  expect(READING_HINT).toBe(
    'AppyTappy reads a value only when its window draws something new. It waits the fastest read while a value changes, and up to the slowest read while it holds still. Each is 1 to 10000 ms.',
  );
  expect(TOLERANCE_HINT).toBe(
    "From 0, an exact match, up to 255, which matches any color. Raise it if the check misses; lower it if it matches when it shouldn't.",
  );
  expect(INK_TOLERANCE_HINT).toBe(
    'From 0, only the exact color, up to 255, which counts every pixel. Raise it if parts of characters go missing; lower it if the background counts as characters.',
  );
});
