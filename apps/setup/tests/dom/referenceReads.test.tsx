import { afterEach, expect, test } from 'bun:test';
import { cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { AppFile } from '../../src/lib/bindings.ts';
import { ReferenceEditor } from '../../src/components/ReferenceEditor.tsx';
import { ReferencePicker } from '../../src/components/ReferencePicker.tsx';
import { coverage } from '../../src/lib/expected.ts';

afterEach(cleanup);

const DIGITS = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

function editor(onReads: (reads: readonly string[]) => void) {
  return (
    <ReferenceEditor
      name="Percent font"
      appName="Player"
      saved={[]}
      labeled={['Lv 87%']}
      recorded={[]}
      reads={DIGITS}
      onReads={onReads}
      readersLose={null}
      coverage={coverage(DIGITS, ['7', '8', '%', 'L', 'v'])}
      lookalikes={[]}
      risksProblem={null}
      finding={[]}
      readers={[]}
      boxes={null}
      findPanel={null}
      record={null}
      unlearned={null}
      onName={() => {}}
    />
  );
}

test('a reference chooses the characters it reads, and says what else it learned it does not read', async () => {
  const chosen: (readonly string[])[] = [];
  render(
    editor((reads) => {
      chosen.push(reads);
    }),
  );
  await userEvent.click(screen.getByRole('checkbox', { name: /Percent/u }));
  expect(chosen).toEqual([[...DIGITS, '%']]);
  expect(screen.getByText('Not among the characters it reads, so it reads none of these:')).toBeTruthy();
});

test('a value that needs a character its reference does not read says so, and counts it as not missing', () => {
  const file: AppFile = {
    format: 2,
    name: 'Player',
    window: { title: '', class: '', executable: '', mustMatch: { title: true, class: false }, fallback: 'none' },
    sizeChanges: 'often',
    antiCheat: 'unknown',
    references: [
      {
        id: 'digits',
        name: 'Digit font',
        origin: { kind: 'byHand' },
        samples: [{ image: 'ref-digits-1.png', text: '1' }],
        characters: '0123456789',
      },
    ],
    values: [],
  };
  render(
    <ReferencePicker
      appName="Player"
      file={file}
      selected="digits"
      newPicked={false}
      onNew={null}
      whole={false}
      characters={['1', '2', '%']}
      onSelect={() => {}}
      onWhole={() => {}}
      onCharacters={() => {}}
    />,
  );
  expect(screen.getByRole('status').textContent).toContain(
    "Digit font doesn't read %, so this value can't either. Edit Digit font on the References page to read it",
  );
  // The 2 is still to learn; the % is not, since teaching it would not make it read.
  const missing = document.querySelector('.missing-note')?.textContent ?? '';
  expect(missing).toContain('2');
  expect(missing).not.toContain('%');
});

test('a value can pick New reference, even while its application has none, and a saved one takes its place', async () => {
  const file: AppFile = {
    format: 2,
    name: 'Player',
    window: { title: '', class: '', executable: '', mustMatch: { title: true, class: false }, fallback: 'none' },
    sizeChanges: 'often',
    antiCheat: 'unknown',
    references: [],
    values: [],
  };
  const picked: string[] = [];
  const picker = (references: AppFile['references'], newPicked: boolean) => (
    <ReferencePicker
      appName="Player"
      file={{ ...file, references }}
      selected="digits"
      newPicked={newPicked}
      onNew={() => {
        picked.push('new');
      }}
      whole={false}
      characters={[]}
      onSelect={(reference) => {
        picked.push(reference);
      }}
      onWhole={() => {}}
      onCharacters={() => {}}
    />
  );
  const { rerender } = render(picker([], false));
  await userEvent.click(screen.getByRole('button', { name: /New reference/u }));
  const digits = { id: 'digits', name: 'Digit font', origin: { kind: 'byHand' as const }, samples: [] };
  rerender(picker([digits], true));
  expect(screen.getByRole('button', { name: /New reference/u }).getAttribute('aria-pressed')).toBe('true');
  // While New reference is picked, the reference the draft names is not shown as picked too.
  expect(screen.getByRole('button', { name: /Digit font/u }).getAttribute('aria-pressed')).toBe('false');
  await userEvent.click(screen.getByRole('button', { name: /Digit font/u }));
  expect(picked).toEqual(['new', 'digits']);
});
