interface Props {
  /** The way to the flow: the page, then each flow that waits under it. */
  readonly trail: readonly string[];
  /** What the flow shows now, such as New value. */
  readonly here: string;
}

/**
 * Where a flow opened for another flow is, such as Automations / New automation / New value. A flow opened from its
 * page shows none, since its heading says as much.
 */
export function Trail({ trail, here }: Props) {
  if (trail.length < 2) {
    return null;
  }
  return (
    <p className="crumb">
      {trail.join(' / ')} / <b>{here}</b>
    </p>
  );
}
