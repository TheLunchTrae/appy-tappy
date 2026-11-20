import { afterEach, expect, test } from 'bun:test';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { AppFile, ReadingRisks, ReferenceDef } from '../../src/lib/bindings.ts';
import { RiskMark } from '../../src/components/RiskMark.tsx';

const font: ReferenceDef = { id: 'font', name: 'Percent font', origin: { kind: 'byHand' }, tolerance: 40, samples: [] };
const file: AppFile = {
  format: 1,
  name: 'Player',
  window: {
    title: 'Player',
    class: 'Window',
    executable: 'Player.exe',
    mustMatch: { title: false, class: false },
    fallback: 'none',
  },
  sizeChanges: 'often',
  antiCheat: 'none',
  references: [font],
  values: [],
};

afterEach(() => {
  cleanup();
  clearMocks();
});

test('a saved reference is marked once its risks answer, and an answer for what it was before is dropped', async () => {
  const answers: PromiseWithResolvers<ReadingRisks>[] = [];
  mockIPC((called) => {
    if (called !== 'saved_reading_risks') {
      return undefined;
    }
    const answer = Promise.withResolvers<ReadingRisks>();
    answers.push(answer);
    return answer.promise;
  });
  const view = render(<RiskMark app="player" file={file} reference={font} />);
  expect(screen.queryByText(/May misread/u)).toBeNull();
  // The reference is saved again before the first answer arrives.
  view.rerender(<RiskMark app="player" file={file} reference={{ ...font, tolerance: 10 }} />);
  expect(answers).toHaveLength(2);
  await act(async () => {
    answers[0]?.resolve({ lookalikes: [['8', '6']], hidesTone: false });
    await Promise.resolve();
  });
  expect(screen.queryByText(/May misread/u)).toBeNull();
  await act(async () => {
    answers[1]?.resolve({ lookalikes: [], hidesTone: true });
    await Promise.resolve();
  });
  expect(screen.getByText('May misread')).not.toBeNull();
  expect(screen.getByRole('tooltip', { hidden: true }).textContent).toBe(
    "An outline or a shadow around the characters isn't part of what it learned. Edit it to see what to do.",
  );
});

test('a check that fails is logged and leaves the reference unmarked, and the next check still marks it', async () => {
  const logged = console.error;
  const errors: unknown[] = [];
  console.error = (error: unknown) => {
    errors.push(error);
  };
  try {
    let failing = true;
    mockIPC(async () =>
      failing ? Promise.reject('Its saved picture is missing.') : { lookalikes: [['8', '6']], hidesTone: false },
    );
    const view = render(<RiskMark app="player" file={file} reference={font} />);
    await act(async () => {
      await Promise.resolve();
    });
    expect(errors).toEqual(['Its saved picture is missing.']);
    expect(screen.queryByText('May misread')).toBeNull();
    // The reference is saved again without the picture it could not learn.
    failing = false;
    view.rerender(<RiskMark app="player" file={file} reference={{ ...font, tolerance: 10 }} />);
    expect((await screen.findByRole('tooltip', { hidden: true })).textContent).toBe(
      '8 and 6 look alike in what it learned. Edit it to see what to do.',
    );
  } finally {
    console.error = logged;
  }
});

test('the mark says why to someone who moves to it with the keyboard', async () => {
  mockIPC(() => ({ lookalikes: [['8', '6']], hidesTone: false }));
  const user = userEvent.setup();
  render(<RiskMark app="player" file={file} reference={font} />);
  const about = await screen.findByRole('button', { name: 'About May misread' });
  expect(screen.queryByRole('tooltip')).toBeNull();

  await user.tab();
  expect(document.activeElement).toBe(about);
  expect(screen.getByRole('tooltip').textContent).toBe(
    '8 and 6 look alike in what it learned. Edit it to see what to do.',
  );
});
