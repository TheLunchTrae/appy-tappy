import { describe, expect, test } from 'bun:test';

import { colorAt, decodePicture, hex } from '../src/lib/pixels.ts';

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
