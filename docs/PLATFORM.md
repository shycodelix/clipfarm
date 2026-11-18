# Platform, packaging and mobile feasibility

## Desktop targets

| Target | UI status | Packaging | Runtime prerequisites |
| --- | --- | --- | --- |
| macOS 12+ (arm64, x86_64) | egui/eframe, Metal/GL via `glow` | `.dmg` via cargo-packager | FFmpeg (Homebrew or bundled); optional whisper.cpp/yt-dlp |
| Windows 10+ (x64, arm64) | egui/eframe, Direct3D/GL | `.msi` / NSIS `.exe` | FFmpeg on PATH (winget/choco/manual); optional tools |
| Linux (x64, arm64) | egui/eframe, X11/Wayland, GL | `.deb`, `.AppImage` | FFmpeg; `libxkbcommon`, Wayland/X11 libs for the window |

clipfarm deliberately does **not** bundle FFmpeg or model weights in the repository.
Installers can add them later; for now the app diagnoses what is missing in **Tuning**
and in `clipfarm doctor`, with per-platform install hints.

### Linux build prerequisites

```bash
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev
```

`eframe` with the `glow` backend uses the system GL driver. On a machine without a GPU,
Mesa's software rasterizer is sufficient; the UI is not doing video compositing.

### Packaging

`crates/clipfarm-app/Cargo.toml` contains `[package.metadata.packager]` for
[cargo-packager](https://github.com/crabnebula-dev/cargo-packager). Build installers
locally or in CI:

```bash
cargo build --release -p clipfarm-app
cargo install cargo-packager --locked
cargo packager --release -p clipfarm-app -f deb     # Linux
cargo packager --release -p clipfarm-app -f dmg     # macOS (run on macOS)
cargo packager --release -p clipfarm-app -f nsis    # Windows (run on Windows)
```

The release workflow in `.github/workflows/release.yml` runs the matching command on
each OS and uploads `dist/`. Signing/notarization secrets are intentionally not
committed; add them as repository secrets when distributing publicly.

**Linux result:** the `.deb` path was exercised end to end here — a 5.0 MB
`clipfarm-app_0.2.0_amd64.deb` with an 11 MB stripped binary, `ar`-valid control and
data archives, a desktop entry, a 1024px icon, and no unresolved shared libraries.
macOS and Windows installers are configured but were not built in this sandbox.

> Container note: cargo-packager writes the running UID into the `ar` header, whose
owner field is six digits. If your build container uses an oversized UID, package inside
`unshare -r` or on the host/CI, otherwise `dpkg-deb` reports a corrupt archive.

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
