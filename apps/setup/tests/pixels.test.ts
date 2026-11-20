import { describe, expect, test } from 'bun:test';

import { PICTURES } from '../src/lib/bindings.ts';
import { type Color, colorAt, cropPicture, decodePicture, hex, tileFor } from '../src/lib/pixels.ts';

function encoded(width: number, height: number, pixels: readonly number[]): ArrayBuffer {
  const buffer = new ArrayBuffer(8 + pixels.length);
  const view = new DataView(buffer);
  view.setUint32(0, width, true);
  view.setUint32(4, height, true);
  new Uint8Array(buffer, 8).set(pixels);
  return buffer;
}

describe('decodePicture', () => {
  test('reads the size and the rows', () => {
    const picture = decodePicture(encoded(2, 1, [1, 2, 3, 255, 4, 5, 6, 255]));
    expect(picture?.width).toBe(2);
    expect(picture?.height).toBe(1);
    expect([...(picture?.data ?? [])]).toEqual([1, 2, 3, 255, 4, 5, 6, 255]);
  });

  test('reads a picture as the Rust side encodes it', () => {
    const { encoded: bytes, width, height, rgba } = PICTURES.sample;
    const picture = decodePicture(new Uint8Array(bytes).buffer);
    expect({ width: picture?.width, height: picture?.height, rgba: [...(picture?.data ?? [])] }).toEqual({
      width,
      height,
      rgba: [...rgba],
    });
  });

  test('finds no picture in an empty answer', () => {
    expect(decodePicture(new ArrayBuffer(0))).toBeNull();
    expect(decodePicture(encoded(0, 0, []))).toBeNull();
  });

  test('refuses rows that do not fit the size', () => {
    expect(decodePicture(encoded(2, 2, [1, 2, 3, 255]))).toBeNull();
  });
});

describe('colorAt', () => {
  const picture = decodePicture(encoded(2, 2, [10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 1, 2, 3, 255]));

  test('reads each pixel in rows', () => {
    if (picture === null) {
      throw new Error('the picture decodes');
    }
    expect(colorAt(picture, 0, 0)).toEqual({ red: 10, green: 20, blue: 30 });
    expect(colorAt(picture, 1, 0)).toEqual({ red: 40, green: 50, blue: 60 });
    expect(colorAt(picture, 0, 1)).toEqual({ red: 70, green: 80, blue: 90 });
    expect(colorAt(picture, 1, 1)).toEqual({ red: 1, green: 2, blue: 3 });
  });

  test('finds nothing outside the picture or between pixels', () => {
    if (picture === null) {
      throw new Error('the picture decodes');
    }
    expect(colorAt(picture, 2, 0)).toBeNull();
    expect(colorAt(picture, 0, 2)).toBeNull();
    expect(colorAt(picture, -1, 0)).toBeNull();
    expect(colorAt(picture, 0.5, 0)).toBeNull();
  });
});

test('hex pads each channel', () => {
  expect(hex({ red: 118, green: 240, blue: 6 })).toBe('#76f006');
  expect(hex({ red: 0, green: 0, blue: 0 })).toBe('#000000');
});

describe('cropPicture', () => {
  // A 3 by 2 picture whose pixels count up from 0, one channel value each.
  const picture = {
    width: 3,
    height: 2,
    data: new Uint8ClampedArray(Array.from({ length: 24 }, (_, index) => Math.floor(index / 4))),
  };

  test('keeps the rows and columns inside the box', () => {
    const crop = cropPicture(picture, { x: 1, y: 0, width: 2, height: 2 });
    expect(crop?.width).toBe(2);
    expect(crop?.height).toBe(2);
    expect([...(crop?.data ?? [])].filter((_, index) => index % 4 === 0)).toEqual([1, 2, 4, 5]);
  });

  test('crops the whole picture to itself', () => {
    expect([...(cropPicture(picture, { x: 0, y: 0, width: 3, height: 2 })?.data ?? [])]).toEqual([...picture.data]);
  });

  test('refuses a box that reaches outside the picture or holds no pixel', () => {
    expect(cropPicture(picture, { x: 2, y: 0, width: 2, height: 1 })).toBeNull();
    expect(cropPicture(picture, { x: 0, y: 1, width: 1, height: 2 })).toBeNull();
    expect(cropPicture(picture, { x: -1, y: 0, width: 1, height: 1 })).toBeNull();
    expect(cropPicture(picture, { x: 0, y: 0, width: 0, height: 1 })).toBeNull();
    expect(cropPicture(picture, { x: 0.5, y: 0, width: 1, height: 1 })).toBeNull();
  });
});

describe('tileFor', () => {
  // The WCAG 2 contrast ratio of two colors.
  const luminance = ({ red, green, blue }: Color): number => {
    const linear = (channel: number): number => {
      const part = channel / 255;
      return part <= 0.04045 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue);
  };
  const contrast = (a: Color, b: Color): number => {
    const [dark, light] = [luminance(a), luminance(b)].toSorted((x, y) => x - y);
    return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
  };

  test('gives every text color a tile it reads on, through the grays where the tile changes', () => {
    const levels = Array.from({ length: 256 }, (_, level) => level);
    const inks: readonly Color[] = levels.flatMap((level) => [
      { red: level, green: level, blue: level },
      { red: level, green: 0, blue: 0 },
      { red: 0, green: level, blue: 0 },
      { red: 255, green: 255, blue: level },
      { red: level, green: 255 - level, blue: 128 },
    ]);
    const unreadable = inks.filter((ink) => contrast(ink, tileFor(ink)) < 4.5).map(hex);
    expect(unreadable).toEqual([]);
  });
});
