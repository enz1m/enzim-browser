# Third-party components

- enzUI-ICED, its iced/winit forks and enzui-webkit: revisions are pinned in
  `DEPENDENCIES.json`; dependency checkouts retain their license files. The
  WebKit SDK maintains the selected native source pin, patches, source bundles
  and native deployment notices. Browser-private historical overlays are not
  applied to the current SDK.
- Brave adblock-rust (`adblock`): Mozilla Public License 2.0. Source:
  https://github.com/brave/adblock-rust. The application uses its library API.
- EasyList and EasyPrivacy data: the EasyList authors, Creative Commons
  Attribution-ShareAlike 3.0 Unported. Pins and notices are in
  `resources/filters/README.md` and `resources/filters/SOURCES.json`.
- uBlock filters and scriptlets: Raymond Hill and contributors,
  GPL-3.0-or-later. Pins are in `resources/filters/COSMETIC_SOURCES.json`;
  license texts and corresponding scriptlet modules are retained alongside
  the generated data.
- Bitwarden's OSS connector: executable and corresponding source archives are
  pinned in `assets/connectors/bitwarden.json` and prepared outside Git.
  License texts and notices are in `assets/connectors/bitwarden/`; binary
  distributions must include the prepared bundle's source and notices.
- Poppler: the Linux desktop build runs the system `pdfinfo` and `pdftoppm`
  tools. The Flatpak builds these tools from the [Poppler release](https://poppler.freedesktop.org/)
  and includes Poppler encoding data. Its `share/doc/enzim-browser/` directory
  contains the Poppler source archive and the notices for both inputs.
- Other Rust dependencies retain the licenses declared by their packages and
  pinned in `Cargo.lock`.
