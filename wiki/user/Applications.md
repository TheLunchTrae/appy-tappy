# Applications

An application is a program whose window AppyTappy watches. Add it before you
[make its values](Values.md#make-a-value), since every value belongs to one.

The **Applications** page in AppyTappy Setup lists each saved application:
how AppyTappy finds its window, its anti-cheat setting, its
[overlay](Overlay.md), and how many of its windows are open now.

## Add, change, or remove an application

- **Add.** Select **Add application**, pick the program's window in the list
  of open windows, and choose what has to match to
  [find it again](#how-appytappy-finds-the-window). Give it a name, answer
  [how often its window changes size](#how-often-the-window-changes-size),
  and select **Save application**.
- **Change.** Select **Edit** on its row to change its name, how its window
  is found, or how often it changes size. Its values stay as they are.
- **Remove.** Select **Remove** on its row and confirm. Its values go with it.
  An application whose values an automation uses can't be removed until that
  automation is changed or deleted.

If AppyTappy can't read a saved application, such as one saved by an older
version, its row says so and AppyTappy never finds its window. Remove it and
add it again.

## How AppyTappy finds the window

AppyTappy finds an application's window by three things.
The ? beside each one in AppyTappy Setup explains it too.

- **Window title** is the name in the window's title bar, such as
  `Example App - Save 3`. Many programs change it as they run, such as to
  show the song or file that is open.
- **Window class** is a hidden type name that Windows gives every kind of
  window, such as `ExampleWindow`. You never see it on screen. It stays the
  same while the program runs, but programs built with the same toolkit can
  share one, so on its own it may not tell two programs apart.
- **Executable** is the program's file, such as `Example.exe`. It always has
  to match, because AppyTappy notices the application starting and closing by
  this program.

When you pick a window, AppyTappy fills in all three from it. Each is a
pattern: `*` matches any run of characters and `?` matches any one character,
and capital letters don't matter. For example, `Example App*` matches the
title `Example App` and also `Example App - Save 3`.

The executable always has to match. Tick **Must match** beside the title or
the class when a window has to match that too to count. At first neither is
ticked, so a title that changes doesn't matter. When several windows count,
AppyTappy uses the one that matches the most of the three, and on a tie the
title counts most, then the class.

**If no window qualifies** shows once the title or the class is ticked. It
says what AppyTappy does when the program is running but none of its windows
matches everything ticked:

- **None** treats the application as closed until a window qualifies. This is
  the default.
- **Any window of the same executable** uses one of the program's other
  windows instead.

While you edit, the list of windows marks the one AppyTappy would use as
**best match**, the others that qualify as **matches**, and those it would take
only as the fallback as **fallback**.

## How often the window changes size

**How often does this window change size?** says how AppyTappy reads the
application's values when its window is a size they weren't drawn at:

- **Often**, recommended for windowed play, scales each value with the
  window. Most applications draw their interface larger as their window grows
  taller, so AppyTappy grows or shrinks the value's region by how much taller
  or shorter the window is, and keeps it as far from the side it
  [stays at](Values.md#window-size). Fix a size only where a value doesn't
  read.
- **Rarely** reads values only at the sizes you drew them at. Nothing is
  guessed, and a new size asks you to draw the value again there. Pick it for
  an application that always runs full screen at one size.

An application saved before AppyTappy asked this reads as **Often**.

## Anti-cheat

Some programs, many online games in particular, have anti-cheat: software that
watches for other programs that interfere with them. AppyTappy reads only
what the window shows, the way a screen recorder does, and changes media and
volume through Windows, so those work for every application.

Pressing keys is different. Anti-cheat can see keys that another program
presses and treat them as cheating, so AppyTappy presses keys only into an
application whose anti-cheat is set to **None**.

Set it in the **Anti-cheat** menu on the Applications page:

- **Not set.** Every new application starts here. Key presses are blocked.
- **User-mode anti-cheat** or **Kernel-level anti-cheat.** The application
  has anti-cheat. Key presses are blocked.
- **None.** The application has no anti-cheat. Key presses are allowed.

AppyTappy asks you to confirm before it sets **None**.

> [!WARNING]
> Only pick **None** if the application has no anti-cheat. Pressing keys into
> an application with anti-cheat can get your account banned.

Changing the setting takes effect at once: automations that press keys into
the application stop when you move it away from **None**.
