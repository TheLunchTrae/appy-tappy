# Screen regions

A screen region source reads a signal from part of an application's window, such
as a progress bar, a number counter, or whether an icon is showing. The setup
app has the user pick the window, take a screenshot, draw a box around the
information, and choose a check. The code lives in `appytappy-sources`:
`screen/image.rs` holds images and regions, `screen/capture.rs` the capture
trait, `screen/check.rs` and `screen/digits.rs` the checks, `screen/saved.rs`
the saved checks, and `schedule.rs` the read interval.
Issue [#5](https://github.com/TheLunchTrae/appy-tappy/issues/5) tracks it.

## Checks

Each check reads a cropped region and produces a signal value. All four are
plain Rust over the region's pixels: they call no operating system API, give
the same answer on every system, and allocate nothing per read.

- **Number** reads digits, a decimal point, and a minus sign. The user types
  the characters a screenshot shows, such as `0123456789.%`, and the check
  learns each character's shape from it. A read finds the ink (pixels near the
  text color), splits it into characters at blank columns, scales each one to
  a fixed grid, and picks the closest learned shape. The text line is the
  band of rows with the most ink, so a stray speck elsewhere in the region
  does not throw it off. A character it has not learned fails the read rather
  than being guessed, unless the number has already ended, as in the "HP" of
  "37% HP". It does not read rotated text, characters that touch, or text on
  a background that shares the text's color; those need a different check.
- **Bar fill** returns how full a bar is, from 0 to 100. The user clicks one
  filled spot and one empty spot. The check counts the region's pixels closer
  to each color and ignores pixels near neither, such as a border or a label:
  a pixel counts only within half the distance between the two colors of the
  nearer one.
- **Color** is yes when the region's average color is within a tolerance of a
  chosen color.
- **Matches reference** is yes when the region looks like a saved picture of
  it, within a tolerance per color channel on average.

On the fixture screenshots, each check takes 2 to 10 microseconds per read on
one CPU core; `mise run rust:bench` prints the current numbers.

Windows has its own OCR engine, and `ocrs` is a pure-Rust neural OCR crate.
Neither is used. Windows OCR would make results depend on the Windows version
and its language packs. `ocrs` adds about 4.5 MB to a release binary and loads
separate neural network model files, to read text that a learned shape match
reads in microseconds. It stays an option for text the number check cannot
read.

## Reference images

A matches-reference check stores its picture as a PNG file. AppyTappy reads
any PNG, converting it to 8-bit color and ignoring alpha, and refuses images
over 8192 pixels on a side. It writes 8-bit RGB.

## Read interval

Each screen binding has a read interval: a minimum and a maximum time between
reads. The defaults are 16 ms and 500 ms. A shorter minimum notices changes
sooner; a longer maximum saves CPU while nothing changes. Reads happen only on
frames the window draws, so a window that draws nothing new is not read at
all.

Between the two, the source adapts. After a read that changed the value, or
that came close to where a rule would fire, it waits the minimum. Each read
that changed nothing doubles the wait, up to the maximum. A bar that holds
still, in a window that keeps drawing, is read about twice a second; one that
moves is read at full speed.

The config format, with a default for all bindings and an override for one,
belongs to [#6](https://github.com/TheLunchTrae/appy-tappy/issues/6) and
[#8](https://github.com/TheLunchTrae/appy-tappy/issues/8).

## Capture

Capture is the one part that needs the operating system, so it sits behind the
`Capture` trait. Windows Graphics Capture, the API screen recorders use, is the
only implementation. The system pushes a frame when the window's content
changes, so a window that does not change costs nothing, and the capture also
asks the system for no more than one frame per minimum interval. Between reads
the capture keeps only the newest frame and copies nothing. When a read is
due, the graphics card copies just the region into a small texture the CPU can
read, and those rows land in a buffer that is reused for every frame. When
the window closes, the capture tells its handler and stops.

Each frame carries the time since the system composed it. The benchmark uses
that to report capture-to-decision latency on a live window:

```sh
APPYTAPPY_BENCH_WINDOW="Example App" APPYTAPPY_BENCH_REGION="0,0,200,20" mise run rust:bench
```

The capture asks Windows to drop the yellow border that marks a captured
window. Windows versions without that option keep the border.
