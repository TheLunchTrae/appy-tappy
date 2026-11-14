// Saving a zip file the setup app makes, the debug bundle or an exported application, where the user picks.

/**
 * Makes a zip with `make` and writes it to the place the user picked. When making or writing it fails, it rejects and
 * leaves no file there, not even one that was there before the user picked it.
 */
export type ZipSaver = (make: () => Promise<Blob>) => Promise<void>;

/** The file the save picker made, as {@link fileSaver} writes it. */
export interface PickedFile {
  readonly createWritable: () => Promise<Pick<FileSystemWritableFileStream, 'write' | 'abort' | 'close'>>;
  readonly remove: () => Promise<void>;
}

/**
 * Saves into `file`, which the picker made, empty, when the user picked it. The picker empties a file that was there
 * before, too, so its old content is gone from the pick on. A zip that fails to be made or written removes the file,
 * so a failed save leaves no file there, not even the one the user picked to replace. The failure is what it rejects
 * with even when the removal fails or throws.
 */
export function fileSaver(file: PickedFile): ZipSaver {
  return async (make) => {
    try {
      const zip = await make();
      const writable = await file.createWritable();
      try {
        await writable.write(zip);
      } catch (error: unknown) {
        await writable.abort();
        throw error;
      }
      await writable.close();
    } catch (error: unknown) {
      try {
        await file.remove();
      } catch {
        // The save's own failure says why nothing was saved; a file left behind is at most an empty one.
      }
      throw error;
    }
  };
}

/** Downloads a zip as `name`, once it is made, in a window without the save picker. */
function downloadSaver(name: string): ZipSaver {
  return async (make) => {
    const zip = await make();
    const link = document.createElement('a');
    const url = URL.createObjectURL(zip);
    link.href = url;
    link.download = name;
    link.click();
    // The download reads the file after the click, so the link goes a little later.
    setTimeout(() => {
      URL.revokeObjectURL(url);
    }, 10_000);
  };
}

/**
 * Asks where to save a zip, with `name` offered. Says `null` when the user closed the picker without picking. A
 * window without the picker downloads the file instead.
 */
async function pickZipFile(name: string): Promise<ZipSaver | null> {
  const picker = window.showSaveFilePicker;
  if (picker === undefined) {
    return downloadSaver(name);
  }
  try {
    const file = await picker.call(window, {
      suggestedName: name,
      types: [{ description: 'Zip file', accept: { 'application/zip': ['.zip'] } }],
    });
    return fileSaver(file);
  } catch (error: unknown) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      return null;
    }
    throw error;
  }
}

/**
 * Asks where to save a zip, with `name` offered, as `pick` does, then makes it with `make` and writes it there. Says
 * false when the user closed the picker, and makes nothing then. It asks first, since the click that asked for it
 * lets the page open the picker for only about 5 seconds and a zip can take longer to make. The picker empties a
 * file it picks, so a zip that fails to be made or written rejects and leaves no file there, not even one the user
 * picked to replace.
 */
export async function saveZip(
  name: string,
  make: () => Promise<Blob>,
  pick: (name: string) => Promise<ZipSaver | null> = pickZipFile,
): Promise<boolean> {
  const write = await pick(name);
  if (write === null) {
    return false;
  }
  await write(make);
  return true;
}
