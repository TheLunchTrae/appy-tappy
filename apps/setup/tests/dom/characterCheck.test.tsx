import { afterEach, beforeEach, expect, test } from 'bun:test';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { fireEvent } from '@testing-library/dom';
import { cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { useState } from 'react';

import { CharacterCheckSheet } from '../../src/components/CharacterCheck.tsx';
import { EMPTY_DRAFT } from '../../src/lib/check.ts';
import { type Place, type Turn, openGuide, settle, turn } from '../../src/lib/confirm.ts';
import { useCharacterCheck } from '../../src/lib/useCharacterCheck.ts';
import { useFound } from '../../src/lib/useFound.ts';
import { useRecording } from '../../src/lib/useRecording.ts';
import { useRecordFocus, useReview } from '../../src/lib/useReview.ts';

const NEEDS: readonly string[] = [];
const PLACES: readonly Place[] = [
  {
    source: 'box',
    index: 0,
    number: 1,
    width: 5,
    characters: [
      { shape: 'a', left: 0, right: 2 },
      { shape: 'b', left: 3, right: 5 },
    ],
    text: '17',
    typed: false,
    keep: true,
    ticks: false,
    reading: false,
    asks: true,
  },
];

/** Apply edits to the fixture's boxes as the reference editor applies them to its draft. */
function Check({ onTurn }: { readonly onTurn: (event: Turn) => void }) {
  const [state, setState] = useState(() => ({
    places: PLACES,
    guide: openGuide(PLACES, NEEDS, { answers: new Map(), left: new Set(), seen: new Set() }).guide,
  }));
  return (
    <>
      <CharacterCheckSheet
        {...state}
        needs={NEEDS}
        picture={() => undefined}
        onEdit={() => undefined}
        onClose={() => undefined}
        onTurn={(event) => {
          onTurn(event);
          setState((current) => {
            const next = turn(current.guide, event, current.places, NEEDS);
            const places = current.places.map((place) => {
              const change = next.changes.find((each) => each.source === place.source && each.index === place.index);
              return change === undefined ? place : { ...place, text: change.text, keep: change.keep };
            });
            return { places, guide: settle(next.guide, places, NEEDS) };
          });
        }}
      />
      <button type="button">Outside the check</button>
    </>
  );
}

beforeEach(() => {
  // The search owner forgets its native search on unmount; these tests start none.
  mockIPC(() => undefined);
});
afterEach(() => {
  cleanup();
  clearMocks();
});

/** The real review and character-check owners, with no captured pictures. */
function Review() {
  const [draft, setDraft] = useState(EMPTY_DRAFT);
  const finder = useFound(draft, () => undefined, null, NEEDS, NEEDS);
  const recorder = useRecording();
  const focus = useRecordFocus();
  const review = useReview(null, draft, setDraft, null, NEEDS, focus);
  const check = useCharacterCheck(finder, review, recorder, NEEDS, {
    boxes: draft.boxes,
    recorded: draft.recorded,
    full: new Map(),
    retype: () => undefined,
    keepReading: () => undefined,
  });
  return (
    <>
      <button type="button" onClick={() => review.show(true)}>
        Open review
      </button>
      <button type="button" onClick={check.open}>
        Check characters
      </button>
      {review.reviewing ? <p>Review open</p> : null}
      {check.guide === null ? null : (
        <CharacterCheckSheet
          guide={check.guide}
          places={check.places}
          needs={NEEDS}
          picture={check.picture}
          onTurn={check.act}
          onEdit={check.edit}
          onClose={check.close}
        />
      )}
    </>
  );
}

test('Escape closes the character check before the review underneath it', async () => {
  const user = userEvent.setup();
  render(<Review />);
  await user.click(screen.getByRole('button', { name: 'Open review' }));
  await user.click(screen.getByRole('button', { name: 'Check characters' }));
  expect(screen.getByRole('heading', { name: 'Nothing to check yet' })).not.toBeNull();
  await user.keyboard('{Escape}');
  expect(screen.queryByRole('heading', { name: 'Nothing to check yet' })).toBeNull();
  expect(screen.getByText('Review open')).not.toBeNull();
  await user.keyboard('{Escape}');
  expect(screen.queryByText('Review open')).toBeNull();
});

test.each(['Enter', 'blur'] as const)('a retype commits once on %s, after typing is complete', async (commit) => {
  const turns: Turn[] = [];
  const user = userEvent.setup();
  render(<Check onTurn={(event) => turns.push(event)} />);
  const input = screen.getByRole<HTMLInputElement>('textbox', { name: 'The character this shape is' });
  await user.click(input);
  await user.keyboard('9');
  expect(turns).toEqual([]);
  expect(input.value).toBe('9');
  if (commit === 'Enter') {
    await user.keyboard('{Enter}');
  } else {
    await user.click(screen.getByRole('button', { name: 'Outside the check' }));
  }
  expect(turns).toEqual([{ kind: 'retype', shape: 'a', character: '9' }]);
  expect(document.activeElement).toBe(screen.getByRole('heading', { name: /Is this 7/ }));
});

test('Escape cancels a typed retype before it can close the character check', async () => {
  const turns: Turn[] = [];
  const user = userEvent.setup();
  render(<Check onTurn={(event) => turns.push(event)} />);
  const input = screen.getByRole<HTMLInputElement>('textbox');
  await user.click(input);
  await user.keyboard('9');
  const escaped = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
  fireEvent(input, escaped);
  expect(escaped.defaultPrevented).toBe(true);
  expect(input.value).toBe('1');
  expect(turns).toEqual([]);
  expect(screen.getByRole('heading', { name: /Is this 1/ })).not.toBeNull();
  await user.click(screen.getByRole('button', { name: 'Outside the check' }));
  expect(turns).toEqual([]);
});

test('a click that blurs a retype never answers the step shown after the blur', async () => {
  const turns: Turn[] = [];
  const user = userEvent.setup();
  render(<Check onTurn={(event) => turns.push(event)} />);
  const answer = screen.getByRole('button', { name: 'Yes, it is 1' });
  await user.click(screen.getByRole('textbox'));
  await user.keyboard('9');
  await user.click(answer);
  expect(answer.isConnected).toBe(false);
  expect(turns).toEqual([{ kind: 'retype', shape: 'a', character: '9' }]);
  expect(screen.getByRole('heading', { name: /Is this 7/ })).not.toBeNull();
  await user.click(screen.getByRole('button', { name: 'Yes, it is 7' }));
  expect(turns).toEqual([{ kind: 'retype', shape: 'a', character: '9' }, { kind: 'confirm' }]);
});
