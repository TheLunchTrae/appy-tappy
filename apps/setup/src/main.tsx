import { getCurrentWindow } from '@tauri-apps/api/window';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App.tsx';
import { Overlay } from './Overlay.tsx';
import { overlayOf } from './lib/overlay.ts';
import './styles.css';

/** The overlay this window shows, or null for the main window, such as outside the app while the UI is worked on. */
function currentOverlay(): ReturnType<typeof overlayOf> {
  try {
    return overlayOf(getCurrentWindow().label);
  } catch {
    return null;
  }
}

const root = document.getElementById('root');
if (root !== null) {
  const overlay = currentOverlay();
  createRoot(root).render(
    <StrictMode>{overlay === null ? <App /> : <Overlay app={overlay.app} mode={overlay.mode} />}</StrictMode>,
  );
}
