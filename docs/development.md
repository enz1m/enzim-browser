# Build and development

## Dependencies

Start with the [README build instructions](../README.md#build-from-source)
and [dependency manifest](../DEPENDENCIES.json). Cargo fetches the pinned
public enzUI, iced, and WebKit adapter sources. The browser and WebKit
adapter use the same enzUI and iced revisions so their types are shared.
The separate SDK checkout supplies the native runtime verifier and launcher.

## Prepare the native runtime

Use the [signed WebKit v0.1.1 release](https://github.com/enz1m/enzim-webkit/releases/tag/v0.1.1).
Its x86-64 archives are qualified for Arch Linux. From the workspace root,
download and verify the native runtime and development SDK before unpacking
them:

```sh
mkdir -p enz_browser-deps
cd enz_browser-deps
gh release download v0.1.1 --repo enz1m/enzim-webkit \
  --pattern 'SHA256SUMS*' \
  --pattern 'enzim-webkit-*-v0.1.1*.tar.xz'

tar -xOf enzim-webkit-native-build-v0.1.1.tar.xz \
  enzim-webkit-native-build-v0.1.1/release-signing-key.pub > release-signing-key.pub
test "$(ssh-keygen -lf release-signing-key.pub | awk '{print $2}')" = \
  'SHA256:mdB+43CW4AixBQ3tFzDkmy56FAPog25xxCPw9KnxRmQ' &&
printf 'enz1m %s\n' "$(cat release-signing-key.pub)" > allowed_signers &&
ssh-keygen -Y verify -f allowed_signers -I enz1m -n file -s SHA256SUMS.sig < SHA256SUMS &&
sha256sum --check --ignore-missing SHA256SUMS &&
tar -xf enzim-webkit-runtime-v0.1.1-archlinux-x86_64.tar.xz &&
tar -xf enzim-webkit-sdk-v0.1.1-archlinux-x86_64.tar.xz
```

For other hosts, see the engine release's native build instructions and
system-library requirements. The installed runtime must match the browser's
engine source pin. The launcher checks the manifest, pin, and native file
hashes before starting the browser.

## Build and run

From the browser checkout:

```sh
python3 tools/prepare_bitwarden.py
cargo fetch --locked
../../enz_browser-deps/enzim-webkit-sdk-v0.1.1-archlinux-x86_64/bin/enzim-webkit-env ./tools/build
./tools/browser https://example.org/
```

Connector preparation verifies the pinned Bitwarden executable and source
archives. The build checks that bundle offline and uses the locked Cargo graph.
Use the same SDK build environment for Rust checks that link WPE.

Set `CARGO_TARGET_DIR` to reuse a compatible build cache. Build and test helpers
use one compiler job, a 3 GiB memory cap, and zero swap through the systemd user
manager.

## Build the Flatpak

Install the shared SDK and Platform from enzim Apps. The Rust compiler comes
from the Freedesktop 26.08 SDK extension on Flathub.

```sh
flatpak remote-add --user --if-not-exists enzimos https://flatpak.enzim.dev/enzimos.flatpakrepo
flatpak install --user enzimos dev.enzim.WebKit.Platform//26.08-1 dev.enzim.WebKit.Sdk//26.08-1
flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//26.08
systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 -- \
  flatpak-builder --user --jobs=1 --sandbox --disable-rofiles-fuse \
    --repo=repo build-flatpak org.enzim.Browser.json
```

Use a tagged Browser checkout. The manifest verifies the release snapshot of the
locked Rust sources and pinned connector assets before the offline build. The
package installs its own credential guard and uses the shared Platform for the
engine library and its subprocesses. It builds Poppler's PDF tools and runs
them in a separate Flatpak sandbox with networking disabled.

## Focused checks

```sh
./tools/test startup
./tools/test credentials
python3 -m unittest discover -s tools -p 'test_*.py'
node tools/check_credential_bridge.cjs
node tools/check_resource_phases.cjs
node tools/test_paint_bundle_labels.cjs
```

`tools/test` uses a disposable XDG environment. Filter Rust tests to the
behavior under review. When adding a regression check, verify that it fails
when the relevant behavior is broken.

## Native and visual checks

Use a disposable profile and dedicated compositor for visual checks. Check
both themes and a 420 × 360 window. For frame or material changes, cover
Wayland compositors with and without blur support and check corners against a
background with visible edges.

Build with `./tools/build --features benchmark` for native diagnostic
entrypoints. Use `ENZIM_QA=1` only in the isolated test session. Run
`./tools/build` afterward to restore the normal feature selection.

| Check | When to run it |
| --- | --- |
| `tools/check_downloads.py` | Download handling, pause/resume, and save flows |
| `tools/check_agent_center.py` | Tab and window identity, page access, and Agent Center routing |
| `tools/check_web_rtc.py` | Media settings, screen sharing, and runtime or sandbox changes |
| `tools/check_passkeys.py` | Passkey scripts, authenticator behavior, and message handlers |
| `tools/check_frame_corners.py` | Window corners and compositor integration |
| `tools/check_hide_rule_invalidation.py` | Cosmetic filtering and style invalidation |
| `tools/check_script_parking.py` | Background script memory and restore |
| `tools/check_script_preload_order.py`, `tools/check_paint_before_deferred.py` | Load ordering and first paint |
| `tools/check_spare_process.py`, `tools/check_web_process_graphics.py` | Renderer startup, ownership, and graphics isolation |

The runners document their dependencies and arguments with `--help`. The
Agent Center and WebRTC runners also need `enzui-center` and `enzui-mcp`
from the Agent Center repository. WebRTC uses generated media and exercises
the renderer and network process boundaries.

For repaint regressions, serve the [input](../tools/fixtures/input-paint.html)
and [drag](../tools/fixtures/drag-paint.html) fixtures on an isolated local
HTTP server. Verify that colours change after release and that the thumb
follows the pointer while held without leaving stale pixels. Use both renderer
paths and try each document palette with `?theme=light` and `?theme=dark`.

Keep raw captures and profiles in the ignored `work/` directory. Use only
disposable profiles and public or synthetic content in documentation images.

## Performance checks

`tools/benchmark_sites.py` compares the browser with Chromium and Firefox on
the [benchmark site list](../tools/benchmark_sites.json). It starts a dedicated
headless Sway session. For example:

```sh
./tools/build --features benchmark
python3 tools/benchmark_sites.py --enzim-binary PATH --rounds 2
```

Use `--only` to select a site, `--browsers enzim` to compare browser variants,
and `--startup N` to measure launch and first paint for a local page.
`tools/benchmark_memory_short.py` and
`tools/benchmark_browser_responsiveness.py` take a dedicated compositor
directory via `--desktop` and use VNC for input. `tools/measure_latency.py`
takes the browser unit, process-ID file, and VNC endpoint explicitly.

Keep timing instrumentation out of memory comparisons. Report browser process
PSS separately from cgroup and GPU totals. See
[memory accounting](memory-accounting.md) for the measurement boundaries.

## Desktop entry for a source checkout

The desktop entry registers links and supported local documents with the same
application ID and MIME declarations as the browser. From a built checkout:

```sh
python3 tools/install_desktop.py
# Or use a specific executable.
python3 tools/install_desktop.py --binary PATH
```

The installer writes the entry, icon, and runtime-aware launcher into your user
directories. It leaves existing defaults in place. Reinstall after moving the
checkout or executable. For binary distribution, see [packaging](publishing.md).

Check the desktop entry and launch handling:

```sh
desktop-file-validate resources/org.enzim.Browser.desktop
./tools/test desktop_open_with
```
