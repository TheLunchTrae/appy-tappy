// Vite bundles the stylesheet that main.tsx imports.
declare module '*.css';

// The save picker of the File System Access API, which WebView2 has and the DOM types leave out.
interface SaveFilePickerOptions {
  readonly suggestedName?: string;
  readonly types?: readonly {
    readonly description: string;
    readonly accept: Readonly<Record<string, readonly string[]>>;
  }[];
}

interface Window {
  readonly showSaveFilePicker?: (options?: SaveFilePickerOptions) => Promise<FileSystemFileHandle>;
}
