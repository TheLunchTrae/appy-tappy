# Screen regions

A screen region source reads a signal from part of an application's window, such
as a progress bar, a number counter, or whether an icon is showing. The setup
app has the user pick the window, take a screenshot, draw a box around the
information, and choose a check. The code lives in `appytappy-sources`:
`screen/image.rs` holds images and regions, `screen/layout.rs` where a
capture packs the regions it copies, `screen/placed.rs` values placed at
one window size and what a frame means to their capture,
`screen/capture.rs` the capture trait
and where a window's content lies in its frame,
`screen/check.rs` and `screen/digits.rs` the checks, `screen/detect.rs` the
proposal of a bar's colors, `screen/mask.rs` and
`screen/select.rs` a state picture's mask and selection, `screen/saved.rs`
the saved checks, `screen/scale.rs` and `screen/resample.rs` how a value
reads at another window size, and `schedule.rs` the read interval.
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
  near the text color), splits it into characters at blank columns, and
  scales each one to a fixed grid by the height of the text line. The text
  line is the band of rows with the most ink, so a stray speck elsewhere in
  the region does not throw it off. Each character takes the closest learned
  shape, if the two are the same but for their edges, as
  [Matching](#matching) describes. A character it has not learned fails the
  read rather than being guessed, unless the number has already ended, as in
  the "HP" of "37% HP", and a read that fails
  [says at which stage](#why-a-number-is-not-read). It does not read rotated
  text, characters that touch, or text on a background that shares the
  text's color; those need a different check. A character drawn as marks
  with blank columns between them, as some fonts draw `%`, is one character
  for each mark. A value may name the characters it reads, `characters` in
  its check, and then reads with only its reference's glyphs for them, as if
  the reference had learned nothing else: a shape takes the closest of those
  glyphs, so a value of digits cannot read a `0` as the `O` its font also
  has, and any other character counts as not learned. `DigitReader::reading`
  drops the other glyphs once, when `saved::build` builds the check, so a
  read matches against fewer glyphs and does nothing more. Characters the
  reference has yet to learn are passed over, and a value whose reference
  knows none of its characters is not read (`SavedError::NoCharacters`).
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

### Matching

An application that scales its interface, or centers a number whose width
changes, draws the same character at another fraction of a pixel each time,
so its edge pixels differ from the ones the reference learned. Counting the
grid cells that differ cannot tell that from another character: an edge that
moves by a fraction of a pixel changes many cells without moving any of
them far, while a different character has cells far from any of the
other's.

So a match looks at where the cells differ. A cell that one shape sets is
apart when the other sets neither that cell nor one beside it, left, right,
above, or below. A moved or thicker edge sets cells beside the other
shape's, so none is apart; a stroke that one character has and the other
lacks is. The closest learned shape has the fewest cells apart, then the
fewest that differ at all, and it matches when at most 1 in 32 of the cells
either shape sets are apart (`CELLS_PER_APART` in `screen/digits.rs`). The
tests draw text as a game does, bold with an outline and anti-aliased at any
fraction of a pixel (`screen/digits/drawn.rs`), and read it at any
fraction and at other sizes from three labeled pictures.

Two limits follow from the grid. Where the text line is less than 16 pixels tall, a
cell is smaller than a pixel, so a moved edge is more than a cell and the
reference needs more labeled pictures. And the grid is scaled by the text
line's height, which is that of the characters the region shows: a
character that reaches above or below the digits, such as a `,`, makes the
line taller and every shape beside it another, so the reference learns the
digits again from pictures that show it.

### Why a number is not read

A read fails at one of four stages, `ReadError` in `screen/digits.rs`, and
its message says which, in the same words in the setup app and in the
runtime's log:

- **No ink.** No pixel in the region has the characters' color.
- **Too many characters.** The region holds more than 24.
- **An unlearned shape.** A character matched no learned shape. The message
  shows every character the read made out, with a `?` for each unlearned
  one, such as `8 7 ?`, and names the learned character the first of them is
  closest to. No number is made of the characters that did match.
- **No number.** Every character matched, and they hold no digits, such as
  `%` alone. The message shows them.

The runtime logs each stage a value fails at (`ReadError::stage`) once
until the value reads again, so a counter that stays unreadable while it
counts is one line, with what its first failed read showed, and a region
that flickers between two stages is two.
[Troubleshooting](Troubleshooting.md#a-number-reads-in-setup-but-the-automation-does-nothing)
says what the user does about each.

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
reads any PNG, converting it to 8-bit color. It refuses a picture over 8192
pixels on a side or with more pixels than `LARGEST_PICTURE`, 7680 by 4320.
Every picture AppyTappy saves is a window's content or part of it. A window
that spans displays can be larger, but AppyTappy chooses this bound, an 8K
display's worth, so a picture's cost stays bounded wherever it came from.
`Image::decode_png` reads the size from the PNG header and refuses a larger
picture before it allocates its pixels, which take about 10 bytes each while
they decode, so a small, highly compressed file cannot claim gigabytes.
Beside the pixels, the decoder may allocate 4 MB, for its rows and the file's
metadata. A sample is read as it looks, ignoring alpha. In a state picture,
alpha is the mask: a pixel at least half opaque is compared,
and a more transparent one is left out. A picture with no transparent pixel is
compared whole, and one with no opaque pixel is refused. AppyTappy writes
8-bit RGB, or RGBA with the left-out pixels fully transparent when a mask
leaves any out, and writes no picture larger than it reads.

## Window content

A capture of a window takes in its frame, title bar and borders included,
but the application draws only in its client area, and only that grows with
the window: a title bar keeps its height in a small window and a large one,
and borderless fullscreen has none. So everything AppyTappy reads is the
window's content, the part of the frame its client area covers. Each frame
the capture copies, the setup app's screenshots, a window's size, a
region's coordinates, and where a click lands are all measured from the
content's corner, and a value drawn in a window with a title bar lands on
the same place in fullscreen.

`screen::content` finds the content in a frame from the frame's bounds on the
screen, which a capture takes in (`DWMWA_EXTENDED_FRAME_BOUNDS`), and the
client area's (`GetWindowInfo`, which gives it in screen pixels whole, also
for a window the system stretches or one laid out right to left), as the
capture reads them for each frame it copies, and `screen::in_frame` places
each region it copies in the frame. `screen::content_on_screen` gives the same
area on the screen, which `System::content_size` and
`System::content_bounds` return. Where the client area cannot be read or
covers none of the frame, the whole frame is the content.

## Window sizes

A region is drawn on a screenshot of one window size. Each application says
how often its window changes size, as `sizeChanges`, and that decides what
its values read at a size they were not drawn at.

- **Often**, the default, scales each value from its first drawing.
  Applications draw their interface larger as their window grows taller, so
  `scale::scaled` scales the region by one factor, the new height over the
  drawn height. The region keeps its scaled distance from the side of the
  window it is pinned to, one of nine: a corner, the middle of an edge, or
  the center. A value saved without a pin stays at the third of the window
  its region's center is in, across and down, and the setup app guesses the
  same way. Since the factor follows the height, the pin down the window
  changes only how the region rounds; across a wider or narrower window it
  moves the region. Each of the region's edges is placed and rounded, a
  half away from zero, and its width and height are the distance between
  them, at least 1 pixel, so a region that touches the side it is pinned to
  still touches it. A region that scales outside the window does not read
  there. Regions are measured from the window's
  [content](#window-content), so a title bar, which keeps its height as the
  window grows, moves nothing. The runtime's log says when a value could not
  be read where its region scales to, and to fix it at that size if the
  region is off.
- **Rarely** reads each value only at the sizes it was drawn at, so nothing
  is guessed.

A **fix** draws a value again at one more window size, up to `MAX_FIXES`.
At that size the value reads at the fix's region, while its application's
size changes either way. A fix of a state check has its own picture of each
state, in the first drawing's order; a fix of a number check may name its
own reference, for characters that look different at that size; a bar or
color check keeps its first drawing's colors and fixes only its region. A
fix cannot be at the size of the first drawing or of another fix, and the
setup app and the runtime refuse such a file with the same checks
(`saved::build`).

`SavedValue::at` resolves where a value reads at a size: its drawing made
there, the first or a fix, or its first drawing scaled, or why it does not
read (`Unread`). A number reads at any size, since a character's shape is
scaled to a fixed grid before it is compared and a match lets its edges
land a cell away; at a size so small that its strokes merge, a fix with a
reference learned at that size reads it. A bar or color check counts
colors, which do not change with size. A state check compares pixel by
pixel, so a scaled region needs its pictures and masks redrawn at its size.
`resample` redraws each axis on its own: shrinking averages the pixels each
new pixel covers, and growing takes the nearest one, so an interface's
sharp edges stay sharp. A mask keeps a pixel when at least half the pixels
it covers are kept, and its color averages only those, so a left-out
background does not tint the thing's edge. A picture redrawn to no pixel
to compare does not read. Redrawing costs one pass over each picture, so
the runtime and the setup app resolve a size once, after the window has
settled at it, and read every frame at that size with the result; a frame
costs what it did before.

A window dragged by its corner passes many sizes. The runtime and the setup
app read nothing at a new size until frames have shown it, and no other,
for `SETTLE` (250 ms, `screen::settle`). Then they resolve it, and start the
capture again at the regions there when some value reads, once for each
settled size. Until a size settles, its values are unseen, so no rule acts
on them. Why a value does not read at a size leaves the size out, so the
log says it once for each reason, not once for each size the window
passes.

A capture is started for one window size, and `FrameMeaning` in
`screen/placed.rs` says what each of its frames means. The runtime's
reader, the overlays and the Values page, the automation editor's live
reading, and the capture screen's live check all ask it, so a frame means
the same to each:

- **Readable.** The window has the size the capture reads at, and the frame
  is read.
- **Resized.** The window has another size, whether the capture copied its
  regions there or they lie outside the window. Nothing is read, and the
  size starts to settle.
- **Unreadable.** The window has the size the capture reads at, and a
  region lies outside it all the same. The reader says so and keeps the
  capture, since one started again would find the same.

Any other frame error is the capture's own: the reader counts it or stops,
as [State machines](State-machines.md) describes for each. `Placed` holds
several values placed at one size with the layout a capture copies for
those that read there, so a reader compares no sizes itself.

State pictures are named after the value and the state's place, such as
`status-1.png`. A fix's pictures, and those of a drawing whose states an
edit took again, are named after the value, a number for the set, and the
state's place, such as `status-2-1.png`. Value IDs have no `-`, so the two
never clash. Each new set takes a number no saved picture of the value
uses, so its pictures never overwrite the ones it replaces, and the setup
app deletes the old ones once the file is saved.

## Content that moves

A check reads one fixed region. Nothing in the runtime looks for a value or
follows it, so content that moves around inside the window, such as a number
that shifts as it gains digits, is read only where the region covers it. A
pin is no help with that: it places the region when the window changes size.

Two checks read their content wherever it is inside the region, so a region
drawn larger than the content, a reading area, covers every place it shows:

- **Number** finds the ink anywhere in the region, so blank margin on any
  side changes nothing. All ink on the text's rows is read as characters,
  left to right, so other ink of the text's color beside the number is read
  with it. A shape the reference has not learned fails the read when it
  comes before the number or right after its digits; once a learned
  character such as `%` has ended the number, a shape after it is skipped.
  The line read is the run of inked rows that holds the most ink, so ink
  above or below the number, with a blank row between, is skipped unless it
  holds more.
- **Bar fill** counts the pixels near its two colors wherever they are: each
  pixel within half the distance between the two colors of the nearer one.
  Any other pixel that near either color counts with the bar's.

**Color** averages the whole region, and **State** compares it pixel by
pixel, so both need their content to stay put in the region. The setup app's
**This content may move** option says this where the region is drawn.

A value saves that its region is a reading area as `readingArea`, which only
a number or bar fill check may have (`AppFile::check`,
`CheckDef::reads_anywhere`). It is setup guidance: the Values page names the
region a reading area, and a fix of the value asks for one again. Nothing
that places or reads the region looks at it, so the value reads the same
with and without it (`saved::build`).

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

## Bar detection

`screen::detect` proposes a bar fill check's two colors, so the user does
not have to click them. It infers only what the bar reader can use. The
reader counts pixels near two colors wherever they are, with no direction,
tolerance, or edge, so the detector's whole answer is two colors. It refuses
a region rather than propose colors the reader would read wrongly. Each
threshold below is a constant at the top of `screen/detect.rs`, which says
what it is for.

- **Layout.** The region's long side is its width, or its height when it is
  taller than wide. Each column along it takes the median color of its
  pixels, which a thin border, a thin label, and shading across the bar do
  not move. The columns' colors are gathered into classes of alike colors
  (`SAME`), and the bar's two colors are the most common class and the most
  common one far enough apart from it (`APART`), each on at least a part's
  worth of columns (`PARTS`, `MIN_PART`).
- **Two plain parts and nothing between.** A column belongs to a part only
  while it is as near the part's color as one plain color stays (`SAME`),
  which is nearer than the reader needs. The columns must then read, from
  the start: the filled part, at most an edge of columns that belong to
  neither (`EDGE`), and the empty part. Columns that belong to neither may
  lie only before and after the two, and take up only so much of the long
  side (`COVER`): a border, a rounded cap, some background. A column of
  another color inside a part refuses the region, so a segmented, striped,
  or fading fill is not proposed.
- **Which color is filled.** The reader cannot tell, so the color at the
  start of the long side is: the left of a wide region, the bottom of a tall
  one. The setup app's **Swap colors** turns it around.
- **The reader agrees.** `BarFill::percent` must read the screenshot within
  `AGREE` points of the fill its layout shows. Otherwise something beside
  the bar has one of the colors, or part of the bar has another shade, and
  the region is refused. Past `ROUGH`
  points the proposal says that readings can be a few points off.
- **Every screenshot.** The colors come from the first screenshot whose
  layout shows a bar, the one the region was drawn on first. Each other
  screenshot must then show the same two colors the same way, as a full, an
  empty, or a partly filled bar that the reader agrees with, or show neither
  color on a part's worth of columns, as a screenshot of something else
  does. One that shows a color but not the bar refuses the region and is
  named: a bar that changes color with its fill or between screenshots
  cannot be read with two colors. Two screenshots that show both parts at
  fills a part apart say that the bar moves, which a picture in two colors
  would not.
- **Around a label.** A label, an icon, or what lies beside a loosely drawn
  bar hides the layout in the columns it covers, though the reader leaves
  its pixels out and reads the bar all the same. So when the whole region
  shows no bar, the detector looks again over only the lanes that each show
  the bar from end to end on their own: two plain parts by the lane's own
  colors, under the same rule as the columns. It takes the lanes that do so
  on every screenshot where enough lanes do, and needs a share of the
  region's lanes to be left (`CLEAR`). Each of those lanes spans the bar by
  itself, so where they lie across the bar does not matter. The same rules
  then hold over those lanes on every screenshot, with the reader still
  reading the whole region. A fill that is striped, segmented, or fading in
  every lane leaves no such lane and stays refused. One that is so in only
  some lanes leaves the others, and is held to the two rules below. The
  proposal says that it went by part of the region.
- **Even spread.** The reader divides the pixels of the filled color by
  those of either color. So what lies over part of the bar, stays there, and
  has neither color shifts the reading by an amount that depends on the
  fill: a label, an icon, a fade in some lanes. A screenshot shows that at
  one fill only, where the reader may well agree. So on every screenshot
  that shows the bar, the pixels the reader counts have to be spread evenly
  along the bar: were the fill to end at any column, the share of the
  counted pixels before that column may differ from the column's place along
  the bar by no more than `AGREE`.
- **Even parts.** What lowers the count in one part only and moves with the
  fill, such as a shine in another shade along the filled part, leaves the
  counted pixels spread evenly while the bar is nearly full or nearly empty.
  So on every screenshot that shows both parts, the detector takes what the
  reader counts in a column of each part: the median over the part's
  columns, the lower of the middle two. A shine over half a part's columns
  or more lowers it, whichever columns it leaves free, while a label, an
  icon, or a rounded end over fewer columns does not, and is held to the
  even spread. From those two counts the detector works out what every fill
  would read, and none may differ from the fill by more than `AGREE`. A
  shine across both parts lowers both counts alike and passes.

A refusal carries a `NoBar` reason. The detector does not find a bar drawn as
separate segments, a striped, textured, or fading fill, a bar with a trail
between its parts, a bar with a shine in another shade over half or more of
one part's columns, a bar under a label or an icon that leaves too few lanes
free or takes the reader
too far off at some fill, or a bar that fills from its middle. A fill that
fades so little along the bar that it stays one plain color is proposed, and
every screenshot must still read with it. Text or marks in one of the bar's
own two colors cannot be told from the bar on a screenshot where they blend
into a part, with detected colors or hand-picked ones. The reader reads some
of those bars with hand-picked colors; supporting them here needs a wider
detector, not another reader.

A shade over fewer than half of a part's columns passes the even parts rule,
so such a bar can be proposed and then read some points off at other fills,
as it does with hand-picked colors.

## Read interval

The runtime reads an application's values in one pass of its reader, so each
reader keeps one read interval for all of them: a minimum and a maximum time
between reads. The defaults are 16 ms and 500 ms. A shorter minimum notices
changes sooner; a longer maximum saves CPU while nothing changes. Reads happen
only on frames the window draws, so a window that draws nothing new is not read
at all.

Between the two, the reader adapts. After a read that changed a value, it waits
the minimum. Each read that changed nothing doubles the wait, up to the maximum.
A bar that holds still, in a window that keeps drawing, is read about twice a
second; one that moves is read at full speed.

The Settings page sets one read interval for every reader, as **Fastest read**
and **Slowest read**, which `settings.json` keeps. A setting in the app
definition, with an override for one binding, belongs to
[#6](https://github.com/TheLunchTrae/appy-tappy/issues/6).

## Capture

Capture is the one part that needs the operating system, so it sits behind the
`Capture` trait. Windows Graphics Capture, the API screen recorders use, is the
only implementation. The system pushes a frame when the window's content
changes, so a window that does not change costs nothing. The capture also asks
the system for no more frames than its handler reads: after a read that waits 64
ms or more, it sets the session's minimum update interval to that wait, never
below the throttle it started with, so a game that repaints all the time while
its values hold still is copied into the frame pool about as often as it is
read. A shorter wait keeps the throttle, so a value that moves does not change
the setting on every read. Windows is expected to hold back a change drawn
inside the interval and deliver it once the interval passes; the footprint
check's settle check tests that. Between reads the capture keeps only the newest
frame and copies nothing. When a read is due, the graphics card copies just the
values' regions into a small texture the CPU can read, stacked one under another
as the capture's `Layout` places them, and those rows land in a buffer that is
reused for every frame. When the window closes, the capture tells its handler
and stops. It does the same when it can no longer produce frames: when the
graphics device is lost, such as after a driver reset or update, and when it
cannot follow the window to a new size. A panic in the capture's frame loop tells
the handler too, with the panic's message, instead of ending the capture's
thread unheard (`run_capture` in `crates/sources/src/screen/capture.rs`). A
closed window and a lost device stop frames rather than fail them, so every
`HEALTH_CHECK`, 2 seconds, whatever arrives meanwhile, the capture checks that
its window is still there and still belongs to the process that owned it at
the start, and that its device still works
(`crates/sources/src/win/capture.rs`). It does not use the capture item's own
Closed event, which would say sooner that the window closed: registering a
handler for it left a thread handle, events and thread pool waits open after
every capture, even once the handler was removed.

Each frame carries the time since the system composed it. The benchmark uses
that to report capture-to-decision latency on a live window:

```sh
APPYTAPPY_BENCH_WINDOW="Example App" APPYTAPPY_BENCH_REGION="0,0,200,20" mise run rust:bench
```

The capture asks Windows to drop the yellow border that marks a captured
window, and each capture asks for leave to drop it. Windows versions without
that option keep the border. A capture that stops, or fails partway through
starting, flushes its Direct3D device, which destroys what it released, and
closes the device too. Each capture's thread joins the Windows
Runtime's multithreaded apartment while it runs and leaves it once it has
released everything, so Windows cleans up the thread's calls before the thread
ends.
