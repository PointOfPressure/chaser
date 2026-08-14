<img src="data/icons/org.chaser.Chaser.svg" align="left" width="64" height="64" alt="Chaser icon">

# Chaser

[![CI](https://github.com/PointOfPressure/chaser/actions/workflows/ci.yml/badge.svg)](https://github.com/PointOfPressure/chaser/actions/workflows/ci.yml)

A GTK4 launcher and manager for [Sober](https://sober.vinegarhq.org/), the way
to play Roblox on Linux. What Bloxstrap is on Windows.

Sober runs the x86-64 Android build of Roblox natively, which sidesteps the
Hyperion anti-cheat that closed off Wine. It is deliberately a minimal runtime,
and VinegarHQ leaves the launcher and config layer to the community. Chaser is
that layer: profiles, a FastFlag catalog, performance presets, launch
orchestration and config backups, without touching the runtime or the Roblox
client.

| The home screen | First-run onboarding |
|---|---|
| ![Play](docs/play.png) | ![Onboarding](docs/onboarding.png) |

## Features

- Launch writes the active profile to Sober first, so what you see is what runs.
- First-run onboarding checks the Sober install and sets a starting preset.
- Profiles: named bundles of graphics mode, renderer, GameMode, RPC, HiDPI and
  gamepad toggles, FastFlags and launch environment.
- Presets: Competitive FPS, Balanced, Cinematic, and Potato for weak GPUs.
- FastFlag catalog, risk-tagged, with a raw JSON editor.
- Performance page: apply a preset, toggle MangoHud, set environment variables.
- Sober's JSONC `config.json` is parsed with its comment header and unmanaged
  keys preserved, written atomically, and backed up before every change to
  `~/.config/chaser/backups`, keeping the newest 20.
- GTK4 and libadwaita. No Electron, no web view.
- CLI with everything the GUI does.

| Profiles editor | FastFlags |
|---|---|
| ![Profiles](docs/profiles.png) | ![FastFlags](docs/fflags.png) |

## Requirements

- [Sober](https://flathub.org/apps/org.vinegarhq.Sober) via Flatpak:
  `flatpak install flathub org.vinegarhq.Sober`
- GTK4 4.14 or newer and libadwaita 1.5 or newer (Ubuntu 24.04, Fedora 40, or
  anything later)
- Rust, to build from source

## Install

### Flatpak

Take `chaser.flatpak` from the
[latest release](https://github.com/PointOfPressure/chaser/releases/latest):

```sh
flatpak install --user ./chaser.flatpak
flatpak run org.chaser.Chaser        # or launch Chaser from the app menu
```

Or build it:

```sh
flatpak install flathub org.gnome.Sdk//49 org.gnome.Platform//49 org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak-builder --user --install --force-clean build-dir packaging/org.chaser.Chaser.yml
```

### From source

```sh
git clone https://github.com/PointOfPressure/chaser.git
cd chaser
# build deps on Debian/Ubuntu/Zorin:
sudo apt install libgtk-4-dev libadwaita-1-dev
cargo build --release
```

Binaries land in `target/release/`: `chaser-gui` is the app, `chaser` the CLI.
Run in place with `./target/release/chaser-gui`, or install:

```sh
sudo install -Dm755 target/release/chaser-gui /usr/local/bin/chaser-gui
sudo install -Dm755 target/release/chaser /usr/local/bin/chaser
install -Dm644 data/org.chaser.Chaser.desktop ~/.local/share/applications/org.chaser.Chaser.desktop
install -Dm644 data/icons/org.chaser.Chaser.svg ~/.local/share/icons/hicolor/scalable/apps/org.chaser.Chaser.svg
```

## CLI

```
chaser status              # Sober install + active profile
chaser init                # create the built-in presets
chaser profiles            # list profiles (* = active)
chaser apply <slug>        # write a profile into Sober's config
chaser launch [--profile <slug>] [--dry-run] [uri]
chaser config [--path]     # print Sober's parsed config
chaser fflags              # list the curated FastFlag catalog
chaser sessions            # recent Sober play sessions
```

## How it works

Chaser does not touch the Roblox client or Sober's runtime. It:

1. reads and writes `~/.var/app/org.vinegarhq.Sober/config/sober/config.json`,
   Sober's own settings surface including its `fflags` object, preserving the
   `// !!! STOP !!!` header and any keys it does not understand;
2. backs the file up before each write, and writes atomically via temp file and
   rename;
3. launches Sober with `flatpak run org.vinegarhq.Sober`, injecting environment
   through `--env=`.

## FastFlags

FastFlags are unsupported by VinegarHQ and can break games. The catalog here is
conservative and every flag is risk-tagged, but use them at your own risk. The
most useful one is `DFIntTaskSchedulerTargetFps`; set it high to uncap the
framerate.

## Not affiliated

An unofficial community project, not affiliated with Roblox or VinegarHQ. It
edits Sober's configuration and launches it. It does not modify the Roblox
client, load modified APKs, or attempt to bypass anti-cheat.

## License

MIT, see [LICENSE](LICENSE).
