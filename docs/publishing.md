# Packaging

A browser package needs the application executable, the matching native WPE
runtime, the credential guard, desktop integration files, and the pinned
Bitwarden bundle. Source builds use the dependency layout in the
[README](../README.md). Use [DEPENDENCIES.json](../DEPENDENCIES.json) and
`Cargo.lock` to identify the inputs.

## Runtime and connector

- Prepare the native runtime and development files with the WebKit SDK's
  packaging tools. Follow the [native runtime verification instructions](development.md#prepare-the-native-runtime)
  and keep the runtime manifest, source pin, and file hashes together.
- Use the SDK's native deployment instructions to expose the runtime and
  mandatory guard to the application.
- Stage the bundled Bitwarden executable, source archive, and notices with
  `python3 tools/prepare_bitwarden.py --offline --install-prefix PREFIX`.
- Preserve the [third-party notices](../THIRD_PARTY.md) and the license
  materials that accompany the native runtime.

## Desktop integration

Use the executable's `--write-desktop-entry PATH` command to export its
application ID and MIME declarations. Supply an `enzim-browser-launch`
command that verifies and supplies the native runtime. See
[desktop entry setup](development.md#desktop-entry-for-a-source-checkout) for
the paths and behavior expected by Linux desktops.
