import { describe, expect, test } from 'bun:test';

import type { CaptureOutline, OutlinedValue } from '../src/lib/bindings.ts';
import { outlineSummary } from '../src/lib/outline.ts';

const box = { x: 0, y: 0, width: 4, height: 4 };

function value(id: string, watched: boolean, reads: boolean): OutlinedValue {
  return {
    id,
    name: id,
    watched,
    region: reads ? box : null,
    problem: reads ? null : 'It does not read here.',
  };
}

function outline(copies: number, values: OutlinedValue[]): CaptureOutline {
  return { copies: Array.from({ length: copies }, () => box), values };
}

describe('outlineSummary', () => {
  test('says when nothing is watched', () => {
    expect(outlineSummary(outline(0, [value('a', false, true)]))).toStartWith('No enabled automation watches');
  });

  test('says when no watched value reads here', () => {
    expect(outlineSummary(outline(0, [value('a', true, false)]))).toStartWith('No watched value reads');
  });

  test('counts the copies and the watched values that read', () => {
    expect(outlineSummary(outline(1, [value('a', true, true), value('b', true, false)]))).toStartWith(
      'At this window size the runtime copies 1 rectangle for 1 watched value.',
    );
    expect(outlineSummary(outline(2, [value('a', true, true), value('b', true, true)]))).toStartWith(
      'At this window size the runtime copies 2 rectangles for 2 watched values.',
    );
  });
});
