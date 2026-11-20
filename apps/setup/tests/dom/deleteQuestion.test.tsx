import { afterEach, expect, test } from 'bun:test';
import { type ReactElement, useState } from 'react';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { AppSummary, AutomationSummary } from '../../src/lib/bindings.ts';
import { AutomationsPage } from '../../src/pages/AutomationsPage.tsx';
import { ReferencesPage } from '../../src/pages/ReferencesPage.tsx';
import { ValuesPage } from '../../src/pages/ValuesPage.tsx';

const app: AppSummary = {
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
    references: [{ id: 'font', name: 'Percent font', origin: { kind: 'byHand' }, tolerance: 40, samples: [] }],
    values: [
      {
        id: 'icon',
        name: 'Playback icon',
        origin: { kind: 'byHand' },
        screen: {
          windowSize: { width: 640, height: 480 },
          region: { x: 4, y: 5, width: 10, height: 10 },
          check: { kind: 'color', color: { red: 255, green: 255, blue: 255 }, tolerance: 10 },
          fixes: [],
        },
      },
    ],
  },
};
const automation: AutomationSummary = {
  automation: {
    id: 'pause',
    name: 'Pause music',
    enabled: true,
    when: { app: 'player', value: 'icon', is: 'equals', target: true },
    actions: [],
    undo: false,
    cooldownMs: 0,
  },
  problem: null,
};

const shared = {
  apps: [app],
  loadError: null,
  onChanged: async () => {},
  onNew: () => undefined,
  drafts: [],
  onFinish: () => undefined,
  onDeleteDraft: async () => {},
};
/** A page with its item saved, or with `gone` once it is deleted and the page has read its files again. */
type Page = (gone?: boolean, onChanged?: () => Promise<void>) => ReactElement;
const PAGES: readonly (readonly [string, string, string, Page])[] = [
  [
    'value',
    'delete_value',
    'Delete value',
    (gone = false, onChanged = shared.onChanged) => (
      <ValuesPage
        {...shared}
        apps={gone ? [] : [app]}
        onChanged={onChanged}
        automations={[]}
        onChange={() => undefined}
      />
    ),
  ],
  [
    'reference',
    'delete_reference',
    'Delete reference',
    (gone = false, onChanged = shared.onChanged) => (
      <ReferencesPage {...shared} apps={gone ? [] : [app]} onChanged={onChanged} onEdit={() => undefined} />
    ),
  ],
  [
    'automation',
    'delete_automation',
    'Delete automation',
    (gone = false, onChanged = shared.onChanged) => (
      <AutomationsPage
        {...shared}
        onChanged={onChanged}
        automations={gone ? [] : [automation]}
        runtime
        onEdit={() => undefined}
      />
    ),
  ],
];

/** `page`, whose item is gone once a delete makes the page read its files again. */
function Deleting({ page }: { readonly page: Page }) {
  const [gone, setGone] = useState(false);
  return page(gone, async () => {
    setGone(true);
  });
}

afterEach(async () => {
  await act(async () => {
    cleanup();
  });
  clearMocks();
});

test.each(PAGES)('deleting a saved %s waits for an answer of its own', async (_kind, command, actLabel, page) => {
  let deletions = 0;
  mockIPC((called) => {
    if (called === command) {
      deletions += 1;
    }
    return undefined;
  });
  const user = userEvent.setup();
  render(page());
  const button = (name: string): HTMLElement => screen.getByRole('button', { name });

  await user.dblClick(button('Delete'));
  expect(deletions).toBe(0);
  // In a window, the second click of a double-click goes to whatever shows where Delete was.
  fireEvent.click(button(actLabel), { detail: 2 });
  expect(deletions).toBe(0);
  expect(document.activeElement).toBe(button('Keep'));

  await user.keyboard('{Enter}');
  expect(document.activeElement).toBe(button('Delete'));
  await user.keyboard('{Enter}{Enter}');
  await user.keyboard(' ');
  await user.keyboard(' ');
  expect(deletions).toBe(0);
  expect(document.activeElement).toBe(button('Delete'));

  await user.click(button('Delete'));
  await user.click(button(actLabel));
  expect(deletions).toBe(1);
});

test.each(PAGES)('deleting a saved %s moves the focus to the page heading', async (_kind, _command, actLabel, page) => {
  mockIPC(() => undefined);
  const user = userEvent.setup();
  render(<Deleting page={page} />);

  await user.click(screen.getByRole('button', { name: 'Delete' }));
  await user.click(screen.getByRole('button', { name: actLabel }));
  // The deleted row takes its buttons with it.
  await waitFor(() => {
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
  expect(document.activeElement).toBe(screen.getByRole('heading', { level: 1 }));
});

test.each(PAGES)(
  'a saved %s is deleted once while its delete is on its way',
  async (_kind, command, actLabel, page) => {
    const pending = Promise.withResolvers<undefined>();
    let deletions = 0;
    mockIPC((called) => {
      if (called === command) {
        deletions += 1;
        return pending.promise;
      }
      return undefined;
    });
    const user = userEvent.setup();
    render(page());

    await user.click(screen.getByRole('button', { name: 'Delete' }));
    const answer = screen.getByRole('button', { name: actLabel });
    fireEvent.click(answer);
    fireEvent.click(answer);
    expect(deletions).toBe(1);
    await act(async () => {
      pending.resolve(undefined);
      await pending.promise;
    });
  },
);

const reader = {
  id: 'percent',
  name: 'Percent',
  origin: { kind: 'byHand' },
  screen: {
    windowSize: { width: 640, height: 480 },
    region: { x: 4, y: 5, width: 10, height: 10 },
    check: { kind: 'number', reference: 'font', whole: false },
    fixes: [],
  },
} as const;
const IN_USE: readonly (readonly [string, string, string, () => ReactElement])[] = [
  [
    'value',
    'delete_value',
    'Pause music uses it. Change or delete that automation first.',
    () => <ValuesPage {...shared} automations={[automation]} onChange={() => undefined} />,
  ],
  [
    'reference',
    'delete_reference',
    'Percent reads with it. Point it at another reference first.',
    () => (
      <ReferencesPage
        {...shared}
        apps={[{ ...app, file: app.file === null ? null : { ...app.file, values: [reader] } }]}
        onEdit={() => undefined}
      />
    ),
  ],
];

test.each(IN_USE)('Delete on a %s in use says what uses it and asks nothing', async (_kind, command, reason, page) => {
  let deletions = 0;
  mockIPC((called) => {
    if (called === command) {
      deletions += 1;
    }
    return undefined;
  });
  const user = userEvent.setup();
  render(page());

  await user.click(screen.getByRole('button', { name: 'Delete' }));
  const notice = screen.getByRole('alertdialog');
  expect(notice.textContent).toContain(reason);
  expect(within(notice).getAllByRole('button')).toHaveLength(1);
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'OK' }));

  await user.keyboard('{Escape}');
  expect(screen.queryByRole('alertdialog')).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Delete' }));
  expect(deletions).toBe(0);
});

test.each(PAGES)('a saved %s whose delete fails says why in its row', async (_kind, command, actLabel, page) => {
  mockIPC(async (called) => (called === command ? Promise.reject('Something else uses it.') : undefined));
  const user = userEvent.setup();
  render(page());

  await user.click(screen.getByRole('button', { name: 'Delete' }));
  await user.click(screen.getByRole('button', { name: actLabel }));
  const why = await screen.findByText('Something else uses it.');
  expect(why.closest('tr')).not.toBeNull();
  expect(screen.queryByRole('button', { name: actLabel })).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'OK' }));

  await user.click(screen.getByRole('button', { name: 'OK' }));
  expect(screen.queryByText('Something else uses it.')).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Delete' }));
});
