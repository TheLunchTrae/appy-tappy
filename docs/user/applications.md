# Applications

An application is a program whose window AppyTappy watches. You add one when
you [make its first value](values.md#make-a-value): picking its window in the
list of open windows saves the application with that value.

The **Applications** page in AppyTappy Setup lists each saved application:
how AppyTappy recognizes its window, its anti-cheat setting, how many of its
windows are open now, and its [overlay](overlay.md).

## How AppyTappy finds the window

AppyTappy recognizes an application's window by three things, the way OBS
does:

- **Window title**, such as `Example App - Save 3`.
- **Window class**, a name the program gives its windows, such as
  `ExampleWindow`.
- **Executable**, the program's file name, such as `Example.exe`.

When you pick a window, AppyTappy fills in all three from it. You can change
them before you save the first value; after that, they stay as they are. Each
is a pattern: `*` matches any run of characters and `?` matches any one
character, and capital letters don't matter. For example, `Example App*`
matches the title `Example App` and also `Example App - Save 3`. As you edit
the patterns, AppyTappy counts the open windows they match.

Many programs change their title as they run, so the title often won't
match later. **When no window matches all three** says what AppyTappy does
then:

- **Only a window that matches all three.**
- **Otherwise one with the same class and executable.**
- **Otherwise any window of the executable.** This is the default.

The executable always has to match.

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
