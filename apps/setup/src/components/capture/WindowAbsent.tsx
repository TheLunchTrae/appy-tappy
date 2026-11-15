import type { ReactNode } from 'react';

import type { WindowChoice } from '../../lib/bindings.ts';
import type { WindowAbsence } from '../../lib/windowSearch.ts';

interface Props {
  readonly absence: WindowAbsence;
  readonly onRefresh: () => void;
  readonly onChoose: (window: WindowChoice) => void;
  /** The way to change how the application is found, where that is a way forward. */
  readonly matchEdit: ReactNode;
}

/**
 * What a capture screen's stage shows in place of the window while none is captured: why, the windows to pick from
 * when several fit, and Refresh, which looks again.
 */
export function WindowAbsent({ absence, onRefresh, onChoose, matchEdit }: Props) {
  return (
    <div className="window-absent">
      <div>
        <div role="status">
          <h2>{absence.heading}</h2>
          <p>{absence.text}</p>
        </div>
        {absence.windows.length === 0 ? null : (
          <ul className="window-list" aria-label="Windows that fit">
            {absence.windows.map((each, index) => (
              <li key={each.handle}>
                <button
                  type="button"
                  data-landing={index === 0 ? 'windows' : undefined}
                  disabled={absence.busy}
                  onClick={() => {
                    onChoose(each);
                  }}
                >
                  <span className="title">{each.title}</span>
                  <small>
                    {each.width} × {each.height}
                  </small>
                  <small>
                    {each.class} · {each.executable}
                  </small>
                  {absence.reads === each.handle ? <small className="ok">Read if it starts now</small> : <small />}
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="row">
          <button
            type="button"
            className="btn primary"
            data-landing="refresh"
            disabled={absence.busy}
            onClick={onRefresh}
          >
            Refresh
          </button>
        </div>
        {matchEdit}
      </div>
    </div>
  );
}
