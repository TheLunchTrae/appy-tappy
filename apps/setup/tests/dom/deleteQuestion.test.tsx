import { afterEach, expect, test } from 'bun:test';
import { type ReactElement, useState } from 'react';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import { DeleteActions } from '../../src/components/DeleteActions.tsx';
import type { AppSummary, AutomationSummary, DraftSummary } from '../../src/lib/bindings.ts';
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
    references: [{ id: 'font', name: 'Percent font', origin: { kind: 'byHand' }, guesses: false, samples: [] }],
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

interface Shared {
  readonly apps: readonly AppSummary[];
  readonly loadError: null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
  readonly drafts: readonly DraftSummary[];
  readonly onFinish: () => void;
  readonly onDeleteDraft: () => Promise<void>;
}
const shared: Shared = {
  apps: [app],
  loadError: null,
  onChanged: async () => {},
  onNew: () => undefined,
  drafts: [],
  onFinish: () => undefined,
  onDeleteDraft: async () => {},
};
/** A page with its item saved, or with `gone` once it is deleted and the page has read its files again. */
type Page = (gone?: boolean, onChanged?: () => Promise<void>, more?: Partial<Shared>) => ReactElement;
const PAGES: readonly (readonly [string, string, string, Page])[] = [
  [
    'value',
    'delete_value',
    'Delete value',
    (gone = false, onChanged = shared.onChanged, more = {}) => (
      <ValuesPage
        {...shared}
        {...more}
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
    (gone = false, onChanged = shared.onChanged, more = {}) => (
      <ReferencesPage {...shared} {...more} apps={gone ? [] : [app]} onChanged={onChanged} onEdit={() => undefined} />
    ),
  ],
  [
    'automation',
    'delete_automation',
    'Delete automation',
    (gone = false, onChanged = shared.onChanged, more = {}) => (
      <AutomationsPage
        {...shared}
        {...more}
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

// What the References page is answered for each reference's mark.
const NO_RISKS = { lookalikes: [] };

/** Answers `command` as `answer` does, and each reference's mark with no risks. */
function mockCommands(answer: (command: string) => unknown = () => undefined): void {
  mockIPC((command) => (command === 'saved_reading_risks' ? NO_RISKS : answer(command)));
}

test.each(PAGES)('deleting a saved %s waits for an answer of its own', async (_kind, command, actLabel, page) => {
  let deletions = 0;
  mockCommands((called) => {
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
  mockCommands();
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
    mockCommands((called) => {
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
    "Can't delete Playback icon: Pause music uses it. Change or delete that automation first.",
    () => <ValuesPage {...shared} automations={[automation]} onChange={() => undefined} />,
  ],
  [
    'reference',
    'delete_reference',
    "Can't delete Percent font: Percent reads with it. Point it at another reference first.",
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
  mockCommands((called) => {
    if (called === command) {
      deletions += 1;
    }
    return undefined;
  });
  const user = userEvent.setup();
  render(page());

  await user.click(screen.getByRole('button', { name: 'Delete' }));
  const notice = screen.getByRole('alertdialog');
  expect(within(notice).getByText(reason)).not.toBeNull();
  expect(within(notice).getAllByRole('button')).toHaveLength(1);
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'OK' }));

  await user.keyboard('{Escape}');
  expect(screen.queryByRole('alertdialog')).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Delete' }));
  expect(deletions).toBe(0);
});

test.each(PAGES)('a saved %s whose delete fails says why in its row', async (_kind, command, actLabel, page) => {
  mockCommands(async (called) => (called === command ? Promise.reject('Something else uses it.') : undefined));
  const user = userEvent.setup();
  render(page());

  await user.click(screen.getByRole('button', { name: 'Delete' }));
  await user.click(screen.getByRole('button', { name: actLabel }));
  const why = await screen.findByText(/Can't delete .+: Something else uses it\./u);
  expect(why.closest('tr')).not.toBeNull();
  expect(screen.queryByRole('button', { name: actLabel })).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'OK' }));

  await user.click(screen.getByRole('button', { name: 'OK' }));
  expect(screen.queryByText(/Something else uses it\./u)).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Delete' }));
});

/** Two rows of a list, which the page asks about one at a time, with the `slow` row's delete in the test's hands. */
function TwoRows({ onDelete, slow = 'First' }: { readonly onDelete: () => Promise<void>; readonly slow?: string }) {
  const [confirming, setConfirming] = useState<string | null>(null);
  return (
    <>
      <input aria-label="Elsewhere" />
      {['First', 'Second'].map((name) => (
        <div key={name} role="group" aria-label={name}>
          <DeleteActions
            name={name}
            actLabel="Delete value"
            confirming={confirming === name}
            onConfirm={(asking) => {
              setConfirming(asking ? name : null);
            }}
            onDelete={name === slow ? onDelete : async () => {}}
          >
            {null}
          </DeleteActions>
        </div>
      ))}
    </>
  );
}

test('a delete that fails after the question moved to another row says why in its own row', async () => {
  const pending = Promise.withResolvers<undefined>();
  const user = userEvent.setup();
  render(<TwoRows onDelete={async () => pending.promise} />);
  const row = (name: string) => within(screen.getByRole('group', { name }));

  await user.click(row('First').getByRole('button', { name: 'Delete' }));
  await user.click(row('First').getByRole('button', { name: 'Delete value' }));
  await user.click(row('Second').getByRole('button', { name: 'Delete' }));
  const keep = row('Second').getByRole('button', { name: 'Keep' });
  expect(document.activeElement).toBe(keep);
  await act(async () => {
    pending.reject('Something else uses it.');
    await pending.promise.catch(() => undefined);
  });

  // The other row's question keeps its place and the focus.
  expect(row('First').getByText("Can't delete First: Something else uses it.")).not.toBeNull();
  expect(row('Second').getByRole('alertdialog', { name: 'Delete Second?' })).not.toBeNull();
  expect(document.activeElement).toBe(keep);

  await user.click(row('First').getByRole('button', { name: 'OK' }));
  expect(screen.queryByText(/Something else uses it/u)).toBeNull();
  expect(document.activeElement).toBe(row('First').getByRole('button', { name: 'Delete' }));
  expect(row('Second').getByRole('alertdialog', { name: 'Delete Second?' })).not.toBeNull();
});

test('a row whose delete is on its way leaves the focus in the question of a row above it', async () => {
  const pending = Promise.withResolvers<undefined>();
  const user = userEvent.setup();
  render(<TwoRows slow="Second" onDelete={async () => pending.promise} />);
  const row = (name: string) => within(screen.getByRole('group', { name }));

  await user.click(row('Second').getByRole('button', { name: 'Delete' }));
  await user.click(row('Second').getByRole('button', { name: 'Delete value' }));
  await user.click(row('First').getByRole('button', { name: 'Delete' }));

  expect(document.activeElement).toBe(row('First').getByRole('button', { name: 'Keep' }));
  await act(async () => {
    pending.resolve(undefined);
    await pending.promise;
  });
});

test('a refusal leaves the focus where the user put it during the delete', async () => {
  const pending = Promise.withResolvers<undefined>();
  const user = userEvent.setup();
  render(<TwoRows onDelete={async () => pending.promise} />);

  await user.click(within(screen.getByRole('group', { name: 'First' })).getByRole('button', { name: 'Delete' }));
  await user.click(screen.getByRole('button', { name: 'Delete value' }));
  const elsewhere = screen.getByRole('textbox', { name: 'Elsewhere' });
  await user.click(elsewhere);
  await act(async () => {
    pending.reject('Something else uses it.');
    await pending.promise.catch(() => undefined);
  });

  expect(screen.getByText("Can't delete First: Something else uses it.")).not.toBeNull();
  expect(document.activeElement).toBe(elsewhere);
});

/**
 * A draft the page lists in a row of its own, whose flow kept nothing the row reads. A value's or a reference's is
 * of an application that is not saved, so its row stays in place when the saved item goes.
 */
function draft(kind: string): DraftSummary {
  if (kind !== 'value' && kind !== 'reference' && kind !== 'automation') {
    throw new Error(`no draft of a ${kind}`);
  }
  return {
    id: 'draft',
    draft: {
      kind,
      app: kind === 'automation' ? null : 'unsaved',
      changes: null,
      name: 'Unfinished',
      missing: [],
      pictures: [],
      state: null,
    },
    problem: null,
    screenshots: 0,
  };
}

test.each(PAGES)(
  'deleting a saved %s clears what the page said about an earlier failure',
  async (kind, _command, actLabel, page) => {
    mockCommands();
    const user = userEvent.setup();
    render(
      <Deleting
        page={(gone, onChanged) =>
          page(gone, onChanged, {
            drafts: [draft(kind)],
            onDeleteDraft: async () => Promise.reject('The draft is open elsewhere.'),
          })
        }
      />,
    );

    await user.click(screen.getByRole('button', { name: 'Delete the draft Unfinished' }));
    await user.click(screen.getByRole('button', { name: 'Delete draft' }));
    expect((await screen.findByRole('alert')).textContent).toBe('The draft is open elsewhere.');

    await user.click(screen.getByRole('button', { name: 'Delete' }));
    await user.click(screen.getByRole('button', { name: actLabel }));
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: actLabel })).toBeNull();
    });
    expect(screen.queryByRole('alert')).toBeNull();
  },
);

test.each(PAGES)(
  'deleting a saved %s leaves the focus in a question that opened meanwhile',
  async (kind, command, actLabel, page) => {
    const pending = Promise.withResolvers<undefined>();
    mockCommands((called) => (called === command ? pending.promise : undefined));
    const user = userEvent.setup();
    render(<Deleting page={(gone, onChanged) => page(gone, onChanged, { drafts: [draft(kind)] })} />);

    await user.click(screen.getByRole('button', { name: 'Delete' }));
    await user.click(screen.getByRole('button', { name: actLabel }));
    await user.click(screen.getByRole('button', { name: 'Delete the draft Unfinished' }));
    const asked = screen.getByRole('alertdialog', { name: 'Delete the draft Unfinished?' });
    const keep = within(asked).getByRole('button', { name: 'Keep' });
    expect(document.activeElement).toBe(keep);
    await act(async () => {
      pending.resolve(undefined);
      await pending.promise;
    });
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: 'Delete' })).toBeNull();
    });
    expect(document.activeElement).toBe(keep);
  },
);
