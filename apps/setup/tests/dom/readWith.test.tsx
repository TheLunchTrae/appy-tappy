import { afterEach, expect, test } from 'bun:test';
import { cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { ReferenceDef } from '../../src/lib/bindings.ts';
import { ReadWith } from '../../src/components/capture/ReadWith.tsx';

afterEach(cleanup);

const reference = (id: string, name: string): ReferenceDef => ({
  id,
  name,
  origin: { kind: 'byHand' },
  guesses: false,
  samples: [],
});

test("a fix reads with its value's reference, listed once and first, or picks another of the application's", async () => {
  const picked: string[] = [];
  render(
    <ReadWith
      references={[reference('big', 'Big digits'), reference('digits', 'Percent font')]}
      shared="digits"
      selected="digits"
      onSelect={(id) => {
        picked.push(id);
      }}
    />,
  );
  const choices = screen.getAllByRole('radio').map((radio) => radio.closest('label')?.getAttribute('aria-label'));
  expect(choices).toEqual(['Percent font', 'Big digits']);
  await userEvent.click(screen.getByRole('radio', { name: 'Big digits' }));
  expect(picked).toEqual(['big']);
});
