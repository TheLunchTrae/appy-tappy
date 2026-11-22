import { afterEach, expect, test } from 'bun:test';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { AppSummary, DraftSummary, ReferenceDef } from '../../src/lib/bindings.ts';
import { type CaptureSetup, keptSetup, storeSetup } from '../../src/lib/captureDraft.ts';
import { EMPTY_DRAFT, NEEDS_REFERENCE } from '../../src/lib/check.ts';
import { type Resumed, keep, saveBeside } from '../../src/lib/drafts.ts';
import { ValuesPage } from '../../src/pages/ValuesPage.tsx';

function player(references: readonly ReferenceDef[]): AppSummary {
  return {
    id: 'player',
    error: null,
    running: null,
    windows: [],
    file: {
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
      references: [...references],
      values: [],
    },
  };
}

const setup: CaptureSetup = {
  valueName: 'Progress',
  step: 'how',
  mayMove: false,
  editing: null,
  draft: { ...EMPTY_DRAFT, region: { x: 4, y: 5, width: 10, height: 10 }, drawnOn: 0, characters: ['1', '%'] },
};
const drafts: readonly DraftSummary[] = [
  {
    id: 'draft-1',
    problem: null,
    screenshots: 1,
    draft: {
      kind: 'value',
      app: 'player',
      changes: null,
      name: 'Progress',
      missing: [NEEDS_REFERENCE],
      pictures: [],
      state: keep('value', storeSetup(setup)),
    },
  },
];
const percent: ReferenceDef = {
  id: 'percent',
  name: 'Percent font',
  origin: { kind: 'byHand' },
  samples: [{ image: 'percent.png', text: '1%' }],
};

afterEach(async () => {
  await act(async () => {
    cleanup();
  });
  clearMocks();
});

function page(references: readonly ReferenceDef[], onFinish: (resumed: Resumed) => void) {
  return (
    <ValuesPage
      apps={[player(references)]}
      automations={[]}
      loadError={null}
      onChanged={async () => {}}
      onNew={() => undefined}
      onChange={() => undefined}
      drafts={drafts}
      onFinish={onFinish}
      onDeleteDraft={async () => {}}
    />
  );
}

test('a value kept without a reference finishes once its application has one, with it picked', async () => {
  mockIPC(() => undefined);
  const finished: Resumed[] = [];
  const { rerender } = render(
    page([], (resumed) => {
      finished.push(resumed);
    }),
  );
  expect(screen.getByText(/Needs a reference\. Make one for Player on the References page/u)).toBeDefined();
  expect(screen.queryByRole('button', { name: 'Finish Progress' })).toBeNull();

  rerender(
    page([percent], (resumed) => {
      finished.push(resumed);
    }),
  );
  expect(screen.getByText('Ready to finish with Percent font.')).toBeDefined();
  await userEvent.click(screen.getByRole('button', { name: 'Finish Progress' }));
  const [opened] = finished;
  const kept = opened === undefined ? null : keptSetup(opened.def);
  expect(opened?.id).toBe('draft-1');
  expect(kept?.step).toBe('how');
  expect(kept?.draft.reference).toBe('percent');
});

test("a reference's draft saved beside a value's goes again when the value's fails", async () => {
  const calls: unknown[] = [];
  mockIPC((command, args) => {
    calls.push([command, args]);
    return command === 'save_draft' ? 'reference-1' : undefined;
  });
  const reference = drafts[0]?.draft ?? null;
  if (reference === null) {
    throw new Error('the draft is set above');
  }
  expect(await saveBeside(reference, async () => ({ id: 'value-1', def: reference }))).toEqual({
    first: { id: 'reference-1', def: reference },
    own: { id: 'value-1', def: reference },
  });
  expect(calls).toEqual([['save_draft', { id: null, draft: reference, screenshots: true }]]);
  calls.length = 0;
  expect(await saveBeside(reference, async () => ({ failed: 'The disk is full.' }))).toBeNull();
  expect(calls).toEqual([
    ['save_draft', { id: null, draft: reference, screenshots: true }],
    ['delete_draft', { id: 'reference-1' }],
  ]);
});
