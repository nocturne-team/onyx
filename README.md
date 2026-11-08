<div align="center">

<img src="src-tauri/icons/128x128@2x.png" alt="Onyx icon: green pixel-art blocks on a dark tile" width="128" height="128">

# Onyx

**A desktop Minecraft launcher and Modrinth client for Windows and Linux.**
Browse and install mods and modpacks, run vanilla, Fabric, Quilt, Forge and NeoForge instances, and see every one of
your own Modrinth projects, drafts and private ones included.

![Platforms: Windows | Linux](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux-62B47A?style=flat-square)
![Loaders: Fabric, Quilt, Forge, NeoForge](https://img.shields.io/badge/loaders-Fabric%20%7C%20Quilt%20%7C%20Forge%20%7C%20NeoForge-DBD0B4?style=flat-square)
![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square)
[![Build](https://img.shields.io/github/actions/workflow/status/nocturne-team/onyx/build.yml?branch=main&style=flat-square&label=build)](https://github.com/nocturne-team/onyx/actions/workflows/build.yml)
![Status: pre-release](https://img.shields.io/badge/status-pre--release-E0A030?style=flat-square)
![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-lightgrey?style=flat-square)

</div>

## Features

- **Every major loader.** Vanilla, Fabric, Quilt, Forge and NeoForge instances. Onyx downloads the right Java for each
  Minecraft version from Mojang, or uses one you point it at
- **Browse Modrinth.** Mods, resource packs, shaders and modpacks, filtered by category, loader and game version.
  Installing into an instance pulls in required dependencies and skips anything you already have
- **Modpacks.** Install them from Browse or import a `.mrpack` file. Updating a pack removes what the old version
  added and leaves the files you added yourself alone
- **Manage what's installed.** Turn mods, resource packs and shaders on and off, and update them from the Content tab
- **Your Modrinth projects.** Sign in with Modrinth and the My projects page lists everything you're a member of,
  including drafts, unlisted, private, rejected and still-in-review projects, and projects owned by your organizations
- **Several Microsoft accounts.** Switch between them from the account menu. Tokens live in your OS keyring, never in a
  config file
- **Looks like home.** Layout follows the Modrinth App. Dark, light and OLED themes, plus a switch that turns off blur
  for slower machines
- **Discord rich presence** for whatever you're playing, off with one toggle in Settings

<!-- Screenshots go here once they're in docs/screenshots/ -->

## Status

Onyx works day to day but has no release builds yet. Signing in with a Microsoft account needs a client ID that
Microsoft has approved for the Minecraft API; Onyx's own is waiting on that review. Until then, build it yourself (see
below) if you want to try it.

Open work is tracked in [`TODO.md`](TODO.md).

## Requirements

|             |                                                                     |
|-------------|---------------------------------------------------------------------|
| **Windows** | Windows 10 or 11 with WebView2 (preinstalled on both)               |
| **Linux**   | `webkit2gtk-4.1`, and a Secret Service keyring such as GNOME Keyring or KWallet |
| **Game**    | A Microsoft account that owns Minecraft: Java Edition               |

## Building

You'll need Rust 1.90+, Node with [pnpm](https://pnpm.io), the Tauri CLI and, on Linux, the WebKitGTK development
package.

```sh
cargo install tauri-cli --version "^2"
pnpm --dir ui install
cargo tauri dev      # run with hot reload
cargo tauri build    # installers end up in target/release/bundle/
```

Logins and rich presence use IDs that are baked in at build time. Put them in a `.env` file at the repo root (real
environment variables win over it). All of them are optional; whatever is missing is turned off in the app.

| Variable                       | What it's for                                                              |
|--------------------------------|----------------------------------------------------------------------------|
| `ONYX_MSA_CLIENT_ID`           | Your Microsoft Entra app (personal accounts, public client, redirect `http://localhost`) |
| `ONYX_MODRINTH_CLIENT_ID`      | Modrinth OAuth app, redirect `http://127.0.0.1:47823/callback`. Without it you can still paste a personal access token |
| `ONYX_MODRINTH_CLIENT_SECRET`  | The secret for that Modrinth app                                           |
| `ONYX_DISCORD_APP_ID`          | Discord application for rich presence                                      |

Checks that CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
pnpm --dir ui check
```

## Layout

```
crates/onyx-core/   Modrinth client, auth, instances, downloads, launching (no Tauri)
src-tauri/          Tauri commands and events, a thin layer over onyx-core
ui/                 Svelte 5 + TypeScript frontend
```

## License

Onyx is released under the [GNU GPL v3.0](LICENSE). The built-in instance icons come from Modrinth's GPL-3.0 assets;
see [their notice](ui/src/assets/instance-icons/NOTICE.md). The app icon is the "blocks" glyph from
[Pixelarticons](https://github.com/halfmage/pixelarticons) (MIT).

Onyx uses the Modrinth API but isn't affiliated with or endorsed by Modrinth. Not an official Minecraft product; not
approved by or associated with Mojang or Microsoft.
