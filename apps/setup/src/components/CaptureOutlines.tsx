import { useEffect, useState } from 'react';

import { captureOutline } from '../lib/api.ts';
import { ask } from '../lib/request.ts';
import type { AppFile, CaptureOutline, RegionDef } from '../lib/bindings.ts';
import { outlineSummary } from '../lib/outline.ts';
import { useSettings } from '../lib/settings.ts';
import type { Size } from '../lib/region.ts';

/**
 * What the runtime copies out of each frame of the saved application `app` at `size`, and where each saved value reads
 * there, while debug details show; null otherwise. It reads again when `file`, the saved file, changes.
 */
export function useCaptureOutline(app: string, size: Size | undefined, file: AppFile | null): CaptureOutline | null {
  const { showDebug } = useSettings().settings;
  const [outline, setOutline] = useState<CaptureOutline | null>(null);
  const width = size?.width ?? 0;
  const height = size?.height ?? 0;
  useEffect(() => {
    setOutline(null);
    if (!showDebug || file === null || width === 0 || height === 0) {
      return undefined;
    }
    return ask(
      async () => captureOutline(app, width, height),
      setOutline,
      () => {
        // An application that is not saved yet has nothing to outline.
      },
    );
  }, [showDebug, app, width, height, file]);
  return outline;
}

/** The outline over a picture of `size` pixels shown at `shown`. */
export function CaptureOutlines({
  outline,
  size,
  shown,
}: {
  readonly outline: CaptureOutline;
  readonly size: Size;
  readonly shown: Size;
}) {
  const scale = size.width > 0 ? shown.width / size.width : 0;
  const place = (region: RegionDef) => ({
    left: region.x * scale,
    top: region.y * scale,
    width: region.width * scale,
    height: region.height * scale,
  });
  return (
    <div className="capture-outlines" aria-hidden="true">
      {outline.copies.map((copy) => (
        <div key={`${copy.x},${copy.y},${copy.width},${copy.height}`} className="copy-outline" style={place(copy)} />
      ))}
      {outline.values.map((value) =>
        value.region === null ? null : (
          <div
            key={value.id}
            className={value.watched ? 'value-outline' : 'value-outline unwatched'}
            style={place(value.region)}
          >
            <span className="outline-tag">{value.name}</span>
          </div>
        ),
      )}
    </div>
  );
}

/** What the outline shows, in words, under the preview. */
export function CaptureOutlineNote({ outline }: { readonly outline: CaptureOutline }) {
  const unread = outline.values.filter((value) => value.problem !== null);
  return (
    <div className="reading-details" aria-label="Capture debug details">
      <p className="debug-line">{outlineSummary(outline)}</p>
      {unread.length === 0 ? null : (
        <ul className="note outline-unread">
          {unread.map((value) => (
            <li key={value.id}>
              {value.name}: {value.problem}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
