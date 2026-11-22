import { afterEach, expect, test } from 'bun:test';
import { cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { AppFile } from '../../src/lib/bindings.ts';
import { ReferenceEditor } from '../../src/components/ReferenceEditor.tsx';
import { ReferencePicker } from '../../src/components/ReferencePicker.tsx';
import { coverage } from '../../src/lib/expected.ts';

afterEach(cleanup);

const DIGITS = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

function editor(valueNeeds: readonly string[], onReads: (reads: readonly string[]) => void) {
  const expected = valueNeeds.length > 0 ? valueNeeds : DIGITS;
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
      valueNeeds={valueNeeds}
      expected={expected}
      coverage={coverage(expected, ['7', '8', '%', 'L', 'v'])}
      lookalikes={[]}
      risksProblem={null}
      finding={[]}
      readers={[]}
      boxes={null}
      findPanel={null}
      record={null}
      unlearned={null}
      onName={() => {}}
      needsField={valueNeeds.length > 0 ? 'editable' : 'hidden'}
      onExpected={() => {}}
    />
  );
}

test('a reference made by itself chooses the characters it reads, and says what else it learned it does not read', async () => {
  const chosen: (readonly string[])[] = [];
  render(
    editor([], (reads) => {
      chosen.push(reads);
    }),
  );
  expect(screen.queryByText('Characters the value needs')).toBeNull();
  await userEvent.click(screen.getByRole('checkbox', { name: /Percent/u }));
  expect(chosen).toEqual([[...DIGITS, '%']]);
  expect(screen.getByText('Not among the characters it reads, so it reads none of these:')).toBeTruthy();
});

test('a reference taught for a value shows its own characters and the value’s apart', () => {
  render(editor(['8', '%'], () => {}));
  expect(screen.getByText('Characters it reads')).toBeTruthy();
  expect(screen.getByText('Characters the value needs')).toBeTruthy();
  // Each chooser has its own field for other characters.
  const ids = screen.getAllByLabelText('Other characters').map((field) => field.id);
  expect(new Set(ids).size).toBe(2);
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
      hasRegion
      whole={false}
      characters={['1', '2', '%']}
      onSelect={() => {}}
      onEdit={() => {}}
      drafted={new Set()}
      onNew={() => {}}
      onRecord={() => {}}
      onWhole={() => {}}
      onCharacters={() => {}}
    />,
  );
  expect(screen.getByRole('status').textContent).toContain(
    "Digit font doesn't read %, so this value can't either. Edit Digit font to read it",
  );
  // The 2 is still to learn; the % is not, since teaching it would not make it read.
  const missing = document.querySelector('.missing-note')?.textContent ?? '';
  expect(missing).toContain('2');
  expect(missing).not.toContain('%');
});
