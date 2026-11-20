import { afterEach, expect, mock, test } from 'bun:test';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import { ReferencePicker } from '../../src/components/ReferencePicker.tsx';
import { SavedStates } from '../../src/components/ValueEdit.tsx';
import type { ValueDef } from '../../src/lib/bindings.ts';
import { EMPTY_DRAFT } from '../../src/lib/check.ts';

afterEach(cleanup);

/** The text of the notes that describe the button called `name`. */
function described(name: string): string {
  const ids = screen.getByRole('button', { name }).getAttribute('aria-describedby') ?? '';
  return ids
    .split(' ')
    .map((id) => document.getElementById(id)?.textContent ?? '')
    .join(' ');
}

test('without a region, the reference buttons say why and do nothing', () => {
  const onNew = mock();
  const onRecord = mock();
  const picker = (hasRegion: boolean) => (
    <ReferencePicker
      appName="Player"
      file={null}
      selected={null}
      hasRegion={hasRegion}
      whole={false}
      characters={[]}
      onSelect={() => undefined}
      onEdit={() => undefined}
      drafted={new Set()}
      onNew={onNew}
      onRecord={onRecord}
      onWhole={() => undefined}
      onCharacters={() => undefined}
    />
  );
  const view = render(picker(false));
  for (const name of ['New reference from this region', 'Record readings']) {
    fireEvent.click(screen.getByRole('button', { name }));
    expect(described(name)).toBe('Draw the region first.');
  }
  expect(onNew).not.toHaveBeenCalled();
  expect(onRecord).not.toHaveBeenCalled();

  view.rerender(picker(true));
  fireEvent.click(screen.getByRole('button', { name: 'New reference from this region' }));
  fireEvent.click(screen.getByRole('button', { name: 'Record readings' }));
  expect(onNew).toHaveBeenCalledTimes(1);
  expect(onRecord).toHaveBeenCalledTimes(1);
});

test('Take the states again says why it waits and does nothing until it can', () => {
  const value: ValueDef = {
    id: 'icon',
    name: 'Playback icon',
    origin: { kind: 'byHand' },
    screen: {
      windowSize: { width: 640, height: 480 },
      region: { x: 4, y: 5, width: 10, height: 10 },
      check: { kind: 'state', tolerance: 10, states: [] },
      fixes: [],
    },
  };
  const onRetake = mock();
  const states = (retakeBlocked: string | null) => (
    <SavedStates
      value={value}
      draft={EMPTY_DRAFT}
      retakeBlocked={retakeBlocked}
      onDraft={() => undefined}
      onRetake={onRetake}
    />
  );
  const view = render(states('Select a screenshot that shows the first state.'));
  fireEvent.click(screen.getByRole('button', { name: 'Take the states again' }));
  expect(described('Take the states again')).toBe('Select a screenshot that shows the first state.');
  expect(onRetake).not.toHaveBeenCalled();

  view.rerender(states(null));
  fireEvent.click(screen.getByRole('button', { name: 'Take the states again' }));
  expect(onRetake).toHaveBeenCalledTimes(1);
});
