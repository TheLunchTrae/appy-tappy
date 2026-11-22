import { afterEach, expect, test } from 'bun:test';
import { cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import { ReferenceEditor } from '../../src/components/ReferenceEditor.tsx';
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
  expect(screen.getAllByLabelText('Other characters')).toHaveLength(2);
});
