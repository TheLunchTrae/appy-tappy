import type { AntiCheatClass } from '../lib/bindings.ts';
import { runningToWarn } from '../lib/antiCheat.ts';

/**
 * Warns that anti-cheat `products` run now, which matters for the actions an application may be given, while the
 * application's anti-cheat `setting` is not set. `setting` is null before an application is chosen.
 */
export function AntiCheatRunning({
  products,
  setting,
}: {
  readonly products: readonly string[];
  readonly setting: AntiCheatClass | null;
}) {
  const warned = runningToWarn(products, setting);
  return warned.length === 0 ? null : (
    <p className="warn">
      Anti-cheat running now: {warned.join(', ')}. Reading the screen is fine, but set the application&apos;s anti-cheat
      before giving it key press or click actions.
    </p>
  );
}
