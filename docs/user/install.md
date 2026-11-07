# Install, update, and uninstall

AppyTappy runs on Windows. It installs for your Windows account only, so it
does not need administrator rights.

## Install

1. Download the installer, the `.exe` file attached to the
   [latest release](https://github.com/TheLunchTrae/appy-tappy/releases/latest).
2. Run it. AppyTappy is not signed, so the first time, Windows shows a
   **Windows protected your PC** warning. Select **More info**, then
   **Run anyway**.
3. Follow the installer's steps. Its last page can open AppyTappy Setup for
   you.

Installing does three things:

- It adds **AppyTappy** to the Start menu. That opens AppyTappy Setup.
- It starts AppyTappy's background part, which shows the
  [tray icon](tray.md).
- It sets the background part to start each time you sign in to Windows.

## Update

Run the installer of the newer version. It closes AppyTappy's background part
first, which normally undoes what your automations changed, such as a lowered
volume. If the background part doesn't close within about 10 seconds, the
installer ends it, and those changes stay. The new one starts when the
installer finishes. Your applications, values, and automations stay as they
are.

## Uninstall

Uninstall AppyTappy from the **Apps** section of Windows Settings, like any
other program. The uninstaller closes the background part and stops it
from starting at sign-in.

Your applications, values, and automations stay on your PC, so a later
install finds them. To remove them too, select **Delete the application data**
in the uninstaller.

If the installer or the uninstaller asks you to close AppyTappy Setup and you
cancel, the background part stays closed until you next sign in.
