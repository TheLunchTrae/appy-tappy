// Frames and screenshots as the setup app's Rust side sends them: the width
// and the height as little-endian 32-bit numbers, then RGBA rows.

/** A picture the UI can draw and sample. */
export interface Picture {
  readonly width: number;
  readonly height: number;
  readonly data: Uint8ClampedArray<ArrayBuffer>;
}

/** A color picked from a picture. */
export interface Color {
  readonly red: number;
  readonly green: number;
  readonly blue: number;
}

const HEADER_BYTES = 8;
const BYTES_PER_PIXEL = 4;

/** The picture in `buffer`, or null when it holds none, as before the first frame arrives. */
export function decodePicture(buffer: ArrayBuffer): Picture | null {
  if (buffer.byteLength < HEADER_BYTES) {
    return null;
  }
  const header = new DataView(buffer, 0, HEADER_BYTES);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const data = new Uint8ClampedArray(buffer, HEADER_BYTES);
  if (width === 0 || height === 0 || data.length !== width * height * BYTES_PER_PIXEL) {
    return null;
  }
  return { width, height, data };
}

/** The color at whole pixel (`x`, `y`), or null outside the picture. */
export function colorAt(picture: Picture, x: number, y: number): Color | null {
  if (!Number.isInteger(x) || !Number.isInteger(y) || x < 0 || y < 0 || x >= picture.width || y >= picture.height) {
    return null;
  }
  const start = (y * picture.width + x) * BYTES_PER_PIXEL;
  const [red, green, blue] = picture.data.subarray(start, start + 3);
  if (red === undefined || green === undefined || blue === undefined) {
    return null;
  }
  return { red, green, blue };
}

/** The color as CSS hex, such as `#76f06a`. */
export function hex(color: Color): string {
  const part = (channel: number): string => channel.toString(16).padStart(2, '0');
  return `#${part(color.red)}${part(color.green)}${part(color.blue)}`;
}
