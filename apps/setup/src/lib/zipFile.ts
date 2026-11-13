// Saving a zip file the setup app made, the debug bundle or an exported application, where the user picks.

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
