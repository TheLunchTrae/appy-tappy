import { type ReactNode, useEffect, useRef } from 'react';

interface Props {
  /** The id of the heading that names the dialog. */
  readonly titleId: string;
  /** The id of the text that says what the dialog does. */
  readonly describedBy: string;
  /** Whether the dialog waits for an answer to what it started, so it stays open until that ends. */
  readonly busy: boolean;
  readonly onClose: () => void;
  readonly children: ReactNode;
}

/**
 * A modal dialog over the page, open while it is mounted. Escape closes it unless it is busy, and focus goes back
 * to whatever opened it, such as its button, once it closes.
 */
export function Dialog({ titleId, describedBy, busy, onClose, children }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    element?.showModal();
    return () => {
      element?.close();
      opener?.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className="card dialog"
      aria-labelledby={titleId}
      aria-describedby={describedBy}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) {
          onClose();
        }
      }}
      onClose={() => {
        // The window can close a dialog without asking, such as on a second Escape. While it is busy it opens again,
        // so the outcome still shows; otherwise the page follows.
        if (busy) {
          dialog.current?.showModal();
        } else {
          onClose();
        }
      }}
    >
      {children}
    </dialog>
  );
}
