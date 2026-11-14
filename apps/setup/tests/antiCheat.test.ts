import { describe, expect, test } from 'bun:test';

import { clickBlocked, keysBlocked, pickAntiCheat, resumedPick, shownAntiCheat } from '../src/lib/antiCheat.ts';

describe('input is sent only where anti-cheat is none', () => {
  const free = { name: 'Notes', antiCheat: 'none' } as const;

  test('keys wait for the watched application to be set to none', () => {
    expect(keysBlocked(free)).toBeNull();
    for (const antiCheat of ['unknown', 'userMode', 'kernelLevel'] as const) {
      expect(keysBlocked({ name: 'Example App', antiCheat })).toBe(
        "Anti-cheat can treat pressed keys as cheating, so keys are pressed only once Example App's anti-cheat is set to None, on the Applications page or when you add or edit it.",
      );
    }
  });

  test('a click names the first application that blocks it, and its setting in words', () => {
    const blocked = (what: string): string =>
      `${what}, so AppyTappy cannot click while it runs: anti-cheat can see clicks that AppyTappy makes. A click needs the watched application and the one it clicks in both set to None, on the Applications page or when you add or edit them.`;
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

describe('the application editor asks about anti-cheat too', () => {
  test('a new application shows not set until a class is picked over it', () => {
    expect(shownAntiCheat(null, null)).toBe('unknown');
    expect(pickAntiCheat('unknown', null)).toBeNull();
    const pick = pickAntiCheat('none', null);
    expect(pick).toEqual({ from: 'unknown', to: 'none' });
    expect(shownAntiCheat(pick, null)).toBe('none');
  });

  test('a saved application shows its class, and picking that class again picks nothing', () => {
    expect(shownAntiCheat(null, 'kernelLevel')).toBe('kernelLevel');
    expect(pickAntiCheat('kernelLevel', 'kernelLevel')).toBeNull();
    expect(pickAntiCheat('none', 'kernelLevel')).toEqual({ from: 'kernelLevel', to: 'none' });
  });

  test("a draft's pick stays only while the class it was picked over is saved", () => {
    const pick = { from: 'unknown', to: 'none' } as const;
    expect(resumedPick(pick, null)).toEqual(pick);
    expect(resumedPick(pick, 'unknown')).toEqual(pick);
    expect(resumedPick(pick, 'kernelLevel')).toBeNull();
    expect(resumedPick(pick, 'none')).toBeNull();
    expect(resumedPick(null, 'none')).toBeNull();
  });
});
