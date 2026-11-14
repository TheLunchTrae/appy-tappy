// Saving a zip file the setup app made, the debug bundle or an exported application, where the user picks.

/** Writes a zip to the place the user picked. */
export type ZipSaver = (zip: Blob) => Promise<void>;

/**
 * Asks where to save a zip, with `name` offered. Says `null` when the user closed the picker without picking. The
 * picker makes the file it picks, empty, so a caller whose zip can fail to be made asks once it is made
 * (`saveZip`); one that asks first opens the picker at once, while the page still has the click that asked for it.
 * A window without the picker downloads the file instead, once it is written.
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

/**
 * Makes a zip with `make`, then asks where to save it, with `name` offered, as `pick` does, and writes it there. Says
 * false when the user closed the picker. A zip that fails to be made rejects before anything is asked, so it leaves
 * no empty file behind.
 */
export async function saveZip(
  name: string,
  make: () => Promise<Blob>,
  pick: (name: string) => Promise<ZipSaver | null> = pickZipFile,
): Promise<boolean> {
  const zip = await make();
  const write = await pick(name);
  if (write === null) {
    return false;
  }
  await write(zip);
  return true;
}
