# Platform, packaging and mobile feasibility

## Desktop targets

| Target | UI status | Packaging | Runtime prerequisites |
| --- | --- | --- | --- |
| macOS 12+ (arm64, x86_64) | egui/eframe, Metal/GL via `glow` | `.dmg` via cargo-packager | FFmpeg (Homebrew or bundled); optional whisper.cpp/yt-dlp |
| Windows 10+ (x64, arm64) | egui/eframe, Direct3D/GL | `.msi` / NSIS `.exe` | FFmpeg on PATH (winget/choco/manual); optional tools |
| Linux (x64, arm64) | egui/eframe, X11/Wayland, GL | `.deb`, `.AppImage` | FFmpeg; `libxkbcommon`, Wayland/X11 libs for the window |

clipfarm bundles **FFmpeg, ffprobe and yt-dlp** inside the Linux and Windows installers
(as external binaries shipped by cargo-packager), and prefers the bundled copy at runtime
before falling back to PATH or an explicit config path. The app still diagnoses missing or
misconfigured tools in **Tuning** and `clipfarm doctor`.

**What is bundled:**
- Linux + Windows: FFmpeg (GPL static build from a **pinned** BtbN/FFmpeg-Builds auto-build
  release, libass included for burned-in captions), ffprobe, and the yt-dlp standalone
  executable. Each binary is fetched and SHA-256-verified against the upstream release
  checksums in CI before packaging. The FFmpeg release is pinned (`FFMPEG_RELEASE` in
  `.github/workflows/release.yml`) so installer contents are reproducible; bump that variable
  to pull in a newer build.
  - Pinned release: `autobuild-2026-11-19-12-39` (newest BtbN auto-build that has a stable
    `autobuild-*` named tag).
  - A newer FFmpeg build exists behind BtbN's floating `latest` tag, but `latest` moves over
    time and its assets use different names, so we do **not** pin to it on purpose. If you want
    to accept that drift, set `FFMPEG_RELEASE=latest` and switch the asset names to the
    `ffmpeg-master-latest-*` form; otherwise leave the pin on the newest named auto-build and
    bump `FFMPEG_RELEASE` when BtbN publishes a newer `autobuild-*` tag.
- macOS: FFmpeg is **not yet bundled**. BtbN/FFmpeg-Builds does not ship macOS builds, so
  macOS keeps the Homebrew fallback until a macOS static source is pinned and a macOS runner
  is available. The runtime resolver already prefers a bundled copy when present, so the
  macOS path is ready once the binary is supplied.

**What is still optional (not bundled into the installer):**
- whisper.cpp binary + model weights — too large and too build-dependent to bake into the
  installer. The app downloads a small model in-app on first use (one-click), and falls back
  to VAD-only when no whisper is available.
- Ollama / local LLM — auto-detected on the loopback, never bundled.

### Linux build prerequisites

```bash
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev
```

`eframe` with the `glow` backend uses the system GL driver. On a machine without a GPU,
Mesa's software rasterizer is sufficient; the UI is not doing video compositing.

### Packaging

`crates/clipfarm-app/Cargo.toml` contains `[package.metadata.packager]` for
[cargo-packager](https://github.com/crabnebula-dev/cargo-packager). Build installers
locally or in CI.

### Local packaging

```bash
cargo build --release -p clipfarm-app
cargo install cargo-packager --locked

# Linux
cargo packager --release -p clipfarm-app -f deb     # produces dist/*.deb

# macOS (run on macOS)
cargo packager --release -p clipfarm-app -f dmg     # produces dist/*.dmg

# Windows (run on Windows)
cargo packager --release -p clipfarm-app -f nsis    # produces dist/*.exe

# Version is read from CARGO_PACKAGER_VERSION (or the app manifest) and used for the
# package/version metadata. In CI it is set explicitly so the workflow and local builds
# agree.
CARGO_PACKAGER_VERSION=0.2.0 cargo packager --release -p clipfarm-app -f dmg
```

### What the packageers produce

| Platform | Format | Tooling | Install scope |
| --- | --- | --- | --- |
| macOS | `.dmg` | cargo-packager (NSIS-less macOS bundle into a DMG) | Drag to Applications (per-user app bundle) |
| Windows | `.exe` NSIS installer | cargo-packager + NSIS | Per-user install; optional CLI shortcut |
| Linux | `.deb` | cargo-packager + dpkg | System package (requires sudo) |

Each installer ships the `clipfarm-app` binary and a desktop integration entry. The CLI
binary (`clipfarm`) is included where the packaging step includes it; per-platform
placement details are in the macOS DMG and Windows installer sections below.

### Windows installer (NSIS)

The Windows package is an NSIS installer produced by cargo-packager. The app bundle, CLI
binary, shortcuts and an uninstaller are all configured in
`crates/clipfarm-app/Cargo.toml` under `[package.metadata.packager]` and the NSIS profile
`crates/clipfarm-app/nsis.toml`.

Current Windows behavior (tuned for a self-contained, user-scoped install):

- **Install mode:** per-user install (`currentUser`), so it does not require administrator
  rights unless the user chooses a system-wide location.
- **Shortcuts:** a Start menu entry (`clipfarm`) and a Desktop shortcut are created; the
  app and CLI can be launched from there.
- **CLI placement:** the CLI binary is included in the install and can be opened from the
  Start menu. Workflow and install profile can be adjusted to also offer a PATH-friendly
  choice; by default the installer does not modify machine PATH, so `clipfarm` is easiest
  to run from the shortcut, the Start menu, or the install directory.
- **Uninstaller:** the NSIS install registers an entry in Add/Remove programs and an
  uninstaller that removes the app, CLI, shortcuts and install directory when invoked.

### macOS DMG

The macOS DMG is a drag-to-Applications image. It does not include a system-wide CLI
install or an installer script beyond the app bundle itself. To also make `clipfarm` CLI
available on PATH on macOS you can either:

- add a post-install script to the DMG that symlinks or copies the CLI into a PATH
  location chosen by the user, or
- ship a separate Homebrew/Linuxbrew-style formula (planned, not yet published).

Until that is added, the macOS route gives you the desktop app; the CLI is available from
the app bundle's `clipfarm` binary or from a source/build checkout.

### Linux `.deb`

The `.deb` installs the app and a desktop entry. Whether the CLI is placed on PATH depends
on the packager profile; the current profile installs the app and puts the binary in the
package so it is available from the install location and the terminal, and registers a
desktop file and icon.

This package was exercised end to end in this repo: a 5.0 MB
`clipfarm-app_0.2.0_amd64.deb` with a stripped binary, `ar`-valid control and data
archives, a desktop entry, a 1024px icon, and no unresolved shared libraries.

> Container note: cargo-packager writes the running UID into the `ar` header, whose
owner field is six digits. If your build container uses an oversized UID, package inside
`unshare -r` or on the host/CI, otherwise `dpkg-deb` reports a corrupt archive.

### Versioning

Package version is derived from the git tag in CI (`v0.2.0` -> `0.2.0`) so the installer
version matches the release. On `workflow_dispatch` from `main` (no tag) the version is
`0.0.0-dev` and you should push a tag to publish real release artifacts. Local builds use
whatever `CARGO_PACKAGER_VERSION` you set, or fall back to the workspace version.

### Signing, notarization and distribution

Signing and notarization secrets are intentionally not committed. When you are distributing
publicly:

- **macOS:** provide a Developer ID Application key and Notarization credentials as
  repository secrets and fill the matching fields in
  `[package.metadata.packager]` (cargo-packager documents the secret names). Until then
  the DMG is unsigned.
- **Windows:** provide a code-signing certificate and its password as secrets and enable
  the NSIS signing step in the packager profile. Until then the NSIS installer is unsigned.
- **Linux:** `.deb` does not require code signing for installation, but you may want to
  upload to a PPA or package repository if you want `apt` fetches to verify the package.

### CI workflow

The release workflow in `.github/workflows/release.yml` runs on any tag matching `v*` and
on `workflow_dispatch`. For each platform/target/format in the matrix it:

1. checks out the repo,
2. installs the Rust toolchain for that target,
3. installs platform prerequisites (FFmpeg + windowing libs),
4. builds `clipfarm-cli` and `clipfarm-app` in release mode for the target,
5. runs `cargo packager` for the chosen format into `dist/`,
6. uploads `dist/` as a workflow artifact named `clipfarm-<format>-<target>`.

When the trigger is a tag, the `attach` job downloads all installer artifacts, flattens
them into a single directory, and attaches every file to the GitHub Release. Artifacts are
also downloadable from the Actions run for any run, including `workflow_dispatch` builds
from `main`.


### Verification status

This rewrite was developed and verified on a headless Linux sandbox:

- `cargo check --workspace` and `cargo test --workspace` pass.
- The CLI was driven end-to-end against a real FFmpeg build: doctor, ingest, plan,
  refine, raw export, 1080×1920 render.
- The integration test fixtures and assertions are in
  `crates/clipfarm-core/tests/ffmpeg_pipeline.rs`.
- The UI was rendered headlessly (Xvfb + Mesa software GL, 1440×900) and captured in
  `docs/screenshots/`. That pass caught and fixed a real layout leak: the clips
  inspector inherited a horizontal layout and ran off-screen. It also exposed
  invisible slider rails and a duplicated clip-card line.
- A real window manager, HiDPI scaling, keyboard focus, file pickers and GPU drivers
  still need a manual pass on macOS, Windows and a desktop Linux session.
- Screenshot mode is a verification aid, not a shipping feature:
  `clipfarm-app --screenshot out.png [--light] [--project <id>] [--prefill <text>]`
  renders one frame and exits. It is how the README images were produced.
- Whisper.cpp, a whisper model and Ollama were not installed, so those paths were
  exercised only through their parsers, guards and graceful-degradation branches.

## Binary strategy

| Component | How clipfarm uses it | Why |
| --- | --- | --- |
| FFmpeg / ffprobe | subprocess, JSON and `-progress` parsing | No C build/link burden, exact cancellation, LGPL/GPL boundary stays with the user's build, hardware encoders appear automatically |
| yt-dlp | subprocess | Moves faster than any binding; format churn is its problem |
| whisper.cpp | subprocess, JSON output | Avoids shipping models and native build scripts; works with the user's existing build |
| Ollama | localhost HTTP | One loopback request format, no SDK, easy to replace with llama.cpp server |

## Mobile feasibility

**Recommendation for v1: do not ship Android or iOS yet.** The engine is portable Rust,
but the product assumptions are desktop-shaped:

- **UI:** `eframe/egui` has no supported Android/iOS path upstream; a mobile UI would be
  a separate renderer (android-activity + egui is experimental, iOS needs a custom
  winit backend). That is a rewrite of the app crate, not a rebuild.
- **FFmpeg:** cross-compiling per ABI is routine, but hardware encode/decode and file
  access need platform glue; iOS forbids spawning arbitrary subprocesses, so the
  subprocess architecture must become an in-process library there.
- **yt-dlp:** depends on a Python runtime and subprocess execution; not viable on iOS,
  fragile on Android.
- **Storage and thermals:** a 2-hour VOD plus intermediates is a poor fit for phones;
  a cloud-free clipping pass is CPU-heavy and will throttle.
- **Distribution:** Apple's rules around downloading/processing third-party video and
  model weights add review risk.

A realistic mobile plan is later and narrower: share clips into the app for **review and
refinement** (Rust core compiled as a library), with ingest/processing on desktop, or a
remote self-hosted compute node. The project JSON is already portable and would be the
sync unit.
