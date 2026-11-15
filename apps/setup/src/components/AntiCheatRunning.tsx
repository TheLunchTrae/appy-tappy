/** Warns that anti-cheat `products` run now, which matters for the actions an application may be given. */
export function AntiCheatRunning({ products }: { readonly products: readonly string[] }) {
  return products.length === 0 ? null : (
    <p className="warn">
      Anti-cheat running now: {products.join(', ')}. Reading the screen is fine, but set the application&apos;s
      anti-cheat before giving it key press or click actions.
    </p>
  );
}
