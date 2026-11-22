import { afterEach, expect, test } from 'bun:test';
import { cleanup, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import { CheckOffer } from '../../src/components/CheckOffer.tsx';

afterEach(cleanup);

test('Check characters is there with nothing unconfirmed, once the reference has a box to check', async () => {
  let opened = 0;
  const offer = (places: number) => (
    <CheckOffer
      characters={[]}
      places={places}
      onCheck={() => {
        opened += 1;
      }}
    />
  );
  const { rerender } = render(offer(0));
  const button = screen.getByRole<HTMLButtonElement>('button', { name: 'Check characters' });
  expect(button.disabled).toBe(true);
  expect(screen.getByText('Draw a box first.')).toBeDefined();
  rerender(offer(2));
  await userEvent.click(screen.getByRole('button', { name: 'Check characters' }));
  expect(opened).toBe(1);
  expect(screen.queryByText('Not confirmed yet')).toBeNull();
});
