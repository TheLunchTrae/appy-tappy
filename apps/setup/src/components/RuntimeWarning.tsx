import { runtimeWarning } from '../lib/runtime.ts';

/** A warning while AppyTappy's background part does not run, given `runs`, or null while that is not known. */
export function RuntimeWarning({ runs }: { readonly runs: boolean | null }) {
  const warning = runtimeWarning(runs);
  return warning === null ? null : (
    <p className="warn" role="status">
      {warning}
    </p>
  );
}
