# Getting started

AppyTappy watches the windows of other programs on your Windows PC and acts
when something in them changes. For example, it can pause your music when a
progress bar fills up, or turn the volume down while a number on screen is
above 90.

In the user guide, an **application** is any program with a window that you want
AppyTappy to watch: a game, a media player, a browser, or anything else.

AppyTappy has two parts:

- **AppyTappy Setup** is the program you open from the Start menu. You use it
  to tell AppyTappy what to watch and what to do. Opening it while it is open
  brings its window to the front.
- **The background part** starts when you sign in and shows an icon in the
  taskbar's notification area. It does the watching and runs your
  automations, whether AppyTappy Setup is open or not.

AppyTappy runs on Windows. It installs for your Windows account only, so it
does not need administrator rights.

## Install

1. Download the installer, the `.exe` file attached to the
   [latest release](https://github.com/TheLunchTrae/appy-tappy/releases/latest).
2. Optionally, check that the installer came from AppyTappy's own release
   before you run it. Install the [GitHub CLI](https://cli.github.com/), then
   run this in the folder you downloaded it to, with your installer's file
   name. It prints **Verification succeeded!** for an installer from a
   release.

   ```sh
   gh attestation verify AppyTappy_0.1.0_x64-setup.exe -R TheLunchTrae/appy-tappy
   ```

3. Run the installer. AppyTappy is not signed, so the first time, Windows shows a
   **Windows protected your PC** warning. Select **More info**, then
   **Run anyway**.
4. Follow the installer's steps. Its last page can open AppyTappy Setup for
   you.

Installing does three things:

- It adds **AppyTappy** to the Start menu. That opens AppyTappy Setup.
- It starts AppyTappy's background part, which shows the
  [tray icon](The-tray-icon.md).
- It sets the background part to start each time you sign in to Windows.

## Make your first automation

1. Open **AppyTappy** from the Start menu.
2. [Add the application](Applications.md): pick the window of the program to
   watch.
3. [Make a value](Values.md): take a screenshot of that window and draw a box
   around what AppyTappy should read, such as a number or a bar.
4. [Make an automation](Automations.md): say when it runs, such as when the
   value is at least 90, and what it does, such as pause the music.

That is all. The background part runs the automation from then on.

A **?** beside a field, heading, or column header in AppyTappy Setup explains
it. Hold the pointer over it, or move to it with Tab, to read the explanation,
and press Esc to hide it.

## Update

When a newer version is out, AppyTappy Setup says so when it opens, at the end
of its tabs, such as **AppyTappy 0.2.0 is available.**

1. Select **Update**. AppyTappy Setup downloads the update while you keep
   working.
2. When it says **AppyTappy 0.2.0 is ready to install.**, select **Install**.
3. Select **Install** again to agree, or **Not now** to keep working.

AppyTappy Setup then closes and installs the update. AppyTappy's background
part closes first, which normally undoes what your automations changed, such as
a lowered volume. Both start again when the update finishes, and your
applications, values, and automations stay as they are. If the download or the
update fails, the notice says why, and **Try again** starts it again.

**Install** waits while you have unsaved work, such as a new value, so save it
or leave it first.

You can also update by running the installer of the newer version from the
[latest release](https://github.com/TheLunchTrae/appy-tappy/releases/latest).
It closes the background part the same way. If the background part doesn't
close within about 10 seconds, the installer ends it, and those changes stay.

## Uninstall

Uninstall AppyTappy from the **Apps** section of Windows Settings, like any
other program. The uninstaller closes the background part and stops it
from starting at sign-in.

Your applications, values, and automations stay on your PC, so a later
install finds them. To remove them too, select **Delete the application data**
in the uninstaller.

If the installer or the uninstaller asks you to close AppyTappy Setup and you
cancel, the background part stays closed until you next sign in.

## Leave before you save

While you make or change an application, a value, a reference, or an
automation, selecting a page, even the one it opened from, first asks whether
to leave without saving when you have changed something, because what you have
not saved is lost. Select **Stay** to carry on, or **Leave** to drop it. While a
save is still under way, the pages, **Leave**, **Change window**, and
**Discard** or **Cancel** wait for it to finish.
