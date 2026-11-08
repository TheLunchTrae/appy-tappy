# Install, update, and uninstall

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
