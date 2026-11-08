# Screen regions

A screen region source reads a signal from part of an application's window, such
as a progress bar, a number counter, or whether an icon is showing. The setup
app has the user pick the window, take a screenshot, draw a box around the
information, and choose a check. The code lives in `appytappy-sources`:
`screen/image.rs` holds images and regions, `screen/capture.rs` the capture
trait, `screen/check.rs` and `screen/digits.rs` the checks, `screen/mask.rs` and
`screen/select.rs` a state picture's mask and selection, `screen/saved.rs`
the saved checks, and `schedule.rs` the read interval.
Issue [#5](https://github.com/TheLunchTrae/appy-tappy/issues/5) tracks it.

## Checks

Each check reads a cropped region and produces a signal value. All four are
plain Rust over the region's pixels: they call no operating system API, give
the same answer on every system, and allocate nothing per read.

- **Number** reads digits, a decimal point, and a minus sign, with a reference
  of the application's: the characters' color and tolerance, and pictures
  labeled with the text they show, such as `0123456789.%`, from which it
  learns each character's shape. Several values read with one reference, and
  each says whether only the whole part counts: a whole value ends the number
  at its `.`, so `87.5%` reads as 87 and needs no glyphs after the point. A
  number that starts with its point reads as `0.`, so `.5` reads as 0.5 and
  `-.5` as -0.5, unless letters came first, as in `Lv.5`, which reads as 5; a
  `.` with no digit after it, as in `...`, is no number. A learned `,` is
  skipped between groups of three digits, so `1,234,567` reads as 1234567. Any
  other character ends the number, so `12/50` reads as 12, and so does a `,`
  that is not followed by exactly three digits, so `12,5` reads as 12 rather
  than guessing that the `,` is a decimal point. A read finds the ink (pixels
  near the text color), splits it into characters at blank columns, scales
  each one to a fixed grid, and picks the closest learned shape. The text line
  is the band of rows with the most ink, so a stray speck elsewhere in the
  region does not throw it off. A character it has not learned fails the read
  rather than being guessed, unless the number has already ended, as in the
  "HP" of "37% HP". It does not read rotated text, characters that touch, or
  text on a background that shares the text's color; those need a different
  check.
- **Bar fill** returns how full a bar is, from 0 to 100. The user clicks one
  filled spot and one empty spot. The check counts the region's pixels closer
  to each color and ignores pixels near neither, such as a border or a label:
  a pixel counts only within half the distance between the two colors of the
  nearer one.
- **Color** is yes when the region's average color is within a tolerance of a
  chosen color.
- **State** reads as the name of the saved picture the region looks most
  like, among those within a tolerance per color channel on average, or
  `none` when no picture is within it. The closest picture wins by its
  average difference, and a tie goes to the earlier state. A picture's mask
  can leave pixels out: its average is then over its kept pixels only, so a
  changing background behind the thing does not count. Each mask becomes runs
  of kept pixel indexes once, when the check loads, so a read walks the runs
  without testing each pixel.

On the fixture screenshots, each check takes 2 to 10 microseconds per read on
one CPU core, and a state check about that much for each of its states;
`mise run rust:bench` prints the current numbers.

Windows has its own OCR engine, and `ocrs` is a pure-Rust neural OCR crate.
The runtime uses neither. Windows OCR would make results depend on the Windows
version and its language packs. `ocrs` adds a few megabytes to a binary and
loads a separate neural network model file, to read text that a learned shape
match reads in microseconds. The setup app uses `ocrs` only to guess the text of
[recorded readings](Setup-app.md#recording) for the user to check.
Reading with it at runtime stays an option for text the number check cannot
read
([#33](https://github.com/TheLunchTrae/appy-tappy/issues/33)).

## Saved pictures

A reference's samples and a state check's pictures are PNG files. AppyTappy
reads any PNG, converting it to 8-bit color, and refuses images over 8192
pixels on a side. A sample is read as it looks, ignoring alpha. In a state
picture, alpha is the mask: a pixel at least half opaque is compared,
and a more transparent one is left out. A picture with no transparent pixel is
compared whole, and one with no opaque pixel is refused. AppyTappy writes
8-bit RGB, or RGBA with the left-out pixels fully transparent when a mask
leaves any out.

## Selection

`screen::select` finds the thing to compare on a screenshot, without a model:

- **Snap** takes a rough box drawn around the thing. The colors along the
  box's edge are the background, the first 256 different ones. Every pixel
  within the tolerance of a background color is left out, wherever it is, so
  background showing inside the thing is left out too; each color in the box
  is compared with the background once. The selection is the rest, trimmed
  to its box.
- **Grow** takes a point. It floods out to the pixels beside it, left, right,
  up, and down, whose colors are within the tolerance of the point's color,
  then fills the holes: unselected pixels that cannot reach the edge of the
  box through other unselected pixels.

Colors are alike when each channel is within the tolerance, as for the color
check. Selections combine with union and difference, and `mask_in` gives the
mask of any region, as after the user adjusts the box by hand.

## Read interval

The runtime reads an application's values in one pass of its reader, so each
reader keeps one read interval for all of them: a minimum and a maximum time
between reads. The defaults are 16 ms and 500 ms. A shorter minimum notices
changes sooner; a longer maximum saves CPU while nothing changes. Reads happen
only on frames the window draws, so a window that draws nothing new is not read
at all.

Between the two, the reader adapts. After a read that changed a value, or
that came close to where a rule would fire, it waits the minimum. Each read
that changed nothing doubles the wait, up to the maximum. A bar that holds
still, in a window that keeps drawing, is read about twice a second; one that
moves is read at full speed.

The Settings page sets one read interval for every reader, as **Fastest read**
and **Slowest read**, which `settings.json` keeps. A setting in the app
definition, with an override for one binding, belongs to
[#6](https://github.com/TheLunchTrae/appy-tappy/issues/6).

## Capture

Capture is the one part that needs the operating system, so it sits behind the
`Capture` trait. Windows Graphics Capture, the API screen recorders use, is the
only implementation. The system pushes a frame when the window's content
changes, so a window that does not change costs nothing, and the capture also
asks the system for no more than one frame per minimum interval. Between reads
the capture keeps only the newest frame and copies nothing. When a read is
due, the graphics card copies just the region into a small texture the CPU can
read, and those rows land in a buffer that is reused for every frame. When
the window closes, the capture tells its handler and stops. It does the same
when it can no longer produce frames: when the graphics device is lost, such
as after a driver reset or update, and when it cannot follow the window to a
new size. A lost device can stop frames altogether, so while no frame comes,
the capture checks its device every `DEVICE_CHECK`
(`crates/sources/src/win/capture.rs`).

Each frame carries the time since the system composed it. The benchmark uses
that to report capture-to-decision latency on a live window:

```sh
APPYTAPPY_BENCH_WINDOW="Example App" APPYTAPPY_BENCH_REGION="0,0,200,20" mise run rust:bench
```

The capture asks Windows to drop the yellow border that marks a captured
window. Windows versions without that option keep the border.
