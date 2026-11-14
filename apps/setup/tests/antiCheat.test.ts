import { describe, expect, test } from 'bun:test';

import { clickBlocked, keysBlocked } from '../src/lib/antiCheat.ts';

describe('input is sent only where anti-cheat is none', () => {
  const free = { name: 'Notes', antiCheat: 'none' } as const;

  test('keys wait for the watched application to be set to none', () => {
    expect(keysBlocked(free)).toBeNull();
    for (const antiCheat of ['unknown', 'userMode', 'kernelLevel'] as const) {
      expect(keysBlocked({ name: 'Example App', antiCheat })).toBe(
        "Anti-cheat can treat pressed keys as cheating, so keys are pressed only once Example App's anti-cheat is set to None on the Applications page.",
      );
    }
  });

  test('a click names the first application that blocks it, and its setting in words', () => {
    const blocked = (what: string): string =>
      `${what}, so AppyTappy cannot click while it runs: anti-cheat can see clicks that AppyTappy makes. A click needs the watched application and the one it clicks in both set to None on the Applications page.`;
    expect(clickBlocked(free, free)).toBeNull();
    expect(clickBlocked(free, null)).toBeNull();
    expect(clickBlocked(free, { name: 'Streaming app', antiCheat: 'userMode' })).toBe(
      blocked("Streaming app's anti-cheat is user-mode"),
    );
    expect(clickBlocked({ name: 'Example App', antiCheat: 'kernelLevel' }, free)).toBe(
      blocked("Example App's anti-cheat is kernel-level"),
    );
    expect(clickBlocked({ name: 'Example App', antiCheat: 'unknown' }, null)).toBe(
      blocked("Example App's anti-cheat is not set"),
    );
    expect(
      clickBlocked({ name: 'Example App', antiCheat: 'unknown' }, { name: 'Streaming app', antiCheat: 'userMode' }),
    ).toBe(blocked("Example App's anti-cheat is not set"));
  });
});
