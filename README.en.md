# Ring Dock

[简体中文](README.md) · English

**Keep your favorite apps, files, and folders in a transparent desktop ring.** Click a sector to open its shortcut panel, then click an item to launch it. Ring Dock is a portable Windows 10/11 x64 desktop utility controlled with the mouse.

**Designed to use little memory.** The ring and panel render only within their visible bounds. The large panel buffer is released when the panel closes, and item icons load on demand. On a Windows 11 test machine at 2560×1392, the formal instance used about **12.7 MiB of private working set** after all categories had been opened twice and closed. Under the same configuration, a before-and-after preview comparison went from **26.9 MiB to 11.9 MiB**, a reduction of about **56%**. Results vary with Windows, icons, and desktop software. Private working set means resident private pages; it is not the process's total working set or a memory guarantee. See the [memory usage notes](docs/memory-usage.md) for measurements and methodology.

<p align="center">
  <img src="docs/images/hero-desktop.png" width="100%" alt="Ring Dock on a fresh mountain desktop" />
</p>

## Download and run

1. Open the **[latest release](https://github.com/stollor/ring-dock/releases/latest)** and download `ring-dock-v...-windows-x64.zip`.
2. Extract it to a folder you can write to, such as `%LOCALAPPDATA%\Programs\RingDock`.
3. Double-click `ring-dock.exe`. Settings are stored in `config.json` beside the executable.

Ring Dock includes its own application icon. To add it to your desktop, right-click `ring-dock.exe` and create a shortcut; the shortcut uses the embedded ring icon.

### Install with an AI assistant and organize desktop shortcuts

Copy the prompt below into an AI assistant that can operate Windows, PowerShell, and your desktop. The installer in this repository downloads and verifies the official release, creates a desktop shortcut with the ring icon, and imports desktop shortcuts. The AI can then help you drag loose files and folders into categories. See the [full installation and icon-organization prompt](docs/AI安装与图标整理提示词.md) for the Chinese version and full details.

````text
Install Ring Dock on my Windows PC and organize my desktop shortcuts by actually using the browser, PowerShell, and desktop controls. Use only https://github.com/stollor/ring-dock. Download and run `tools/install/install.ps1` from the same latest release, and follow its verification, installation, and shortcut-import steps. Preserve any existing `config.json`. If Windows asks for UAC permission, explain that it is for the optional WinMemoryCleaner helper and wait for me; do not approve it for me.

After importing shortcuts, drag loose files and folders from the desktop root into the matching expanded panels. Do not run or open items individually, move them, or scan outside the desktop. Choose category icons based on the items in each category. Report the install path, number of imported shortcuts, category count, and config backup path. Ask whether to keep `config.json` before uninstalling.

```powershell
$repo = 'stollor/ring-dock'
$tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
$script = Join-Path $env:TEMP 'ring-dock-install.ps1'
Invoke-WebRequest "https://raw.githubusercontent.com/$repo/$tag/tools/install/install.ps1" -OutFile $script
& $script
```
````

> On first launch, if WinMemoryCleaner is installed, Ring Dock asks once for administrator permission to start its optional memory-cleanup helper. Declining does not prevent the ring or panels from working; it only disables memory cleanup.

## Information on the ring

- The **cyan progress ring** shows total CPU usage; the **purple ring** shows physical memory usage.
- The **small dot** marks seconds and moves smoothly around the clock once per minute.
- **Weather** is fetched at startup using the Windows system location, when available. You can also select “Locate and update weather” in Settings. It refreshes every 30 minutes. The center background draws simple sun, cloud, rain, snow, and wind effects based on the weather and time of day.
- **Click the center** to clean files older than 24 hours from the current user's temporary folders. Files in use and reparse points are skipped. If the optional WinMemoryCleaner helper is available, Ring Dock also asks it to trim low-priority standby memory. The result appears on the ring.

## Click a sector to open its collection

Each sector opens its own category. The panel below shows the saved apps and shortcuts in the “AI Assistant” category. Category names, icons, and saved items are customizable.

<p align="center">
  <img src="docs/images/ai-assistant-panel.png" width="520" alt="Ring Dock panel with saved AI assistant apps" />
</p>

By default, panel items use their original Windows icons, and ring categories show icons without text. Right-click the ring to open Settings. You can enable “Start Ring Dock when I sign in to Windows” and change category display, category icons, panel icon style, clock format, opacity, and layout. Category icons can be selected individually or matched automatically for collaboration, development, AI, entertainment, apps, files, folders, and websites.

<p align="center">
  <img src="docs/images/settings.png" width="420" alt="Ring Dock category and icon settings" />
</p>

## What makes it useful

- **Lives on the desktop.** The normal window is embedded in the Windows desktop and does not stay above the apps you are using.
- **Per-pixel transparency.** Wallpaper shows through the ring, which has a translucent glass appearance. Live background blur is not provided.
- **Desktop icons keep working.** Clicks on transparent parts pass through to the desktop below.
- **Saves shortcuts and paths.** Adding or removing a favorite does not move or delete its source file.
- **Local settings.** Favorites and personal configuration are not uploaded.
- **Low memory use.** Compact rendering bounds, on-demand icons, and releasing the panel buffer when closed help keep memory use down. See the measurement above and the [memory usage notes](docs/memory-usage.md).

## Release contents

Pushing a version tag beginning with `v` triggers a Windows build in GitHub Actions. Each release includes a Windows x64 ZIP and a SHA-256 checksum file. The download contains Ring Dock only. WinMemoryCleaner is optional; the ring and shortcut panels work without it.

## Build from source

The development environment needs Windows, the Rust MSVC toolchain, and the Windows SDK resource compiler:

```powershell
cargo build --release --locked
```

The executable is written to `target\release\ring-dock.exe`. The application icon resource is `assets\ring.ico`.

## Project notes

Ring Dock is a Windows desktop utility built with Rust, Direct2D, DirectWrite, and layered windows. Windows 11 is the primary test environment. Other Windows versions, desktop customization software, and multi-monitor DPI combinations have not all been individually verified. See [current capabilities and limitations](docs/当前能力与限制.md).

This repository has not declared an open-source license. You may download and run release packages; public visibility alone does not grant permission to copy, modify, or redistribute the source code.

For questions or feature requests, visit [GitHub Issues](https://github.com/stollor/ring-dock/issues).
