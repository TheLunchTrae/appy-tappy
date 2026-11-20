import { afterEach, expect, test } from 'bun:test';
import type { ReactElement } from 'react';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
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
const PAGES: readonly (readonly [string, string, string, () => ReactElement])[] = [
  [
    'value',
    'delete_value',
    'Delete value',
    () => <ValuesPage {...shared} automations={[]} onChange={() => undefined} />,
  ],
  ['reference', 'delete_reference', 'Delete reference', () => <ReferencesPage {...shared} onEdit={() => undefined} />],
  [
    'automation',
    'delete_automation',
    'Delete automation',
    () => <AutomationsPage {...shared} automations={[automation]} runtime onEdit={() => undefined} />,
  ],
];

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
