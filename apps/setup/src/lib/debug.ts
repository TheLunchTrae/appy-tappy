/** What the Debug page shows, in the words and units the user reads. */

/** A time in nanoseconds, as `0.05 ms`, `9.4 ms`, `2400 ms` or `12.3 s`. */
export function durationText(ns: number): string {
  const ms = ns / 1e6;
  if (ms >= 10_000) {
    return `${(ms / 1000).toFixed(1)} s`;
  }
  if (ms >= 100) {
    return `${ms.toFixed(0)} ms`;
  }
  return `${ms.toFixed(ms < 1 ? 2 : 1)} ms`;
}

/** A time since a trace started, in nanoseconds, as `0:07.204` or `1:02:03.004`. */
export function elapsedText(ns: number): string {
  const totalMs = Math.floor(ns / 1e6);
  const ms = totalMs % 1000;
  const totalSeconds = Math.floor(totalMs / 1000);
  const seconds = totalSeconds % 60;
  const totalMinutes = Math.floor(totalSeconds / 60);
  const minutes = totalMinutes % 60;
  const hours = Math.floor(totalMinutes / 60);
  const tail = `${String(seconds).padStart(2, '0')}.${String(ms).padStart(3, '0')}`;
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, '0')}:${tail}` : `${minutes}:${tail}`;
}

/** A file's size, as `800 bytes`, `12 KB` or `4.2 MB`. */
export function sizeText(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} bytes`;
  }
  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 1024)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** The name a debug bundle made at `when` is saved under, such as `appytappy-debug-2026-11-12-1430.zip`. */
export function bundleName(when: Date): string {
  const two = (part: number): string => String(part).padStart(2, '0');
  const day = `${when.getFullYear()}-${two(when.getMonth() + 1)}-${two(when.getDate())}`;
  return `appytappy-debug-${day}-${two(when.getHours())}${two(when.getMinutes())}.zip`;
}

/** Writes a zip to the place the user picked. */
export type ZipSaver = (zip: Blob) => Promise<void>;

/**
 * Asks where to save a zip, with `name` offered, before the zip is made, so the picker opens at once while the page
 * still has the click that asked for it. Says `null` when the user closed the picker without picking. A window
 * without the picker downloads the file instead, once it is written.
 */
export async function pickZipFile(name: string): Promise<ZipSaver | null> {
  const picker = window.showSaveFilePicker;
  if (picker === undefined) {
    return async (zip) => {
      const link = document.createElement('a');
      const url = URL.createObjectURL(zip);
      link.href = url;
      link.download = name;
      link.click();
      // The download reads the file after the click, so the link goes a little later.
      setTimeout(() => {
        URL.revokeObjectURL(url);
      }, 10_000);
      await Promise.resolve();
    };
  }
  let file;
  try {
    file = await picker.call(window, {
      suggestedName: name,
      types: [{ description: 'Zip file', accept: { 'application/zip': ['.zip'] } }],
    });
  } catch (error: unknown) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      return null;
    }
    throw error;
  }
  return async (zip) => {
    const writable = await file.createWritable();
    try {
      await writable.write(zip);
    } catch (error: unknown) {
      // Leaves the picked file as it was rather than half written.
      await writable.abort();
      throw error;
    }
    await writable.close();
  };
}
