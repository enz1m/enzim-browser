# Bundled filter data

EasyList and EasyPrivacy by the EasyList authors (https://easylist.to/).
Their source repository is https://github.com/easylist/easylist.

These data files use the upstream Creative Commons Attribution-ShareAlike 3.0
Unported license, under the repository's dual-license choice:
https://creativecommons.org/licenses/by-sa/3.0/
Upstream license notice: https://easylist.to/pages/licence.html

`SOURCES.json` records the exact source commit and content hashes. The files
assemble the authors' template includes and replace the timestamp marker;
filter expressions are otherwise preserved. Run `python3 tools/update_filters.py`
to prepare a new pinned snapshot from the public source repository.

The browser code's MIT license does not replace the filter data license.

Cosmetic filtering also uses the pinned uBlock main and Quick fixes lists from
uBlockOrigin/uAssets and scriptlets from gorhill/uBlock.
`COSMETIC_SOURCES.json` records their source commits and generated-data hashes. Their GPL-3.0-or-later notices are distributed
in `uassets-LICENSE` and `ublock-LICENSE`; the corresponding scriptlet source
modules are in `scriptlet-source/`. These are third-party data and code with
their own license, separate from the browser's MIT sources.

Rebuild the same snapshot with `python3 tools/update_cosmetics.py
--assets-commit <recorded commit> --scriptlets-commit <recorded commit>`.
The builder recursively assembles filter includes and selects desktop WebKit
conditions. The recorded input hashes and capabilities make that selection
reproducible; unknown conditions and recursive includes fail the update.
Run `python3 tools/test_filter_assets.py` to check condition and include handling.
The builder evaluates only the scriptlet source module registration and serializes each
registered function, aliases, dependencies and trust mask. The browser supplies
the scriptlets' private `scriptletGlobals` map and injects host-selected scripts
at document start. Cosmetic selectors are applied in an isolated world, with
bounded class/id surveys for dynamically added content. Per-site exceptions
turn off network, cosmetic and scriptlet protection together.

Native rule-conversion counts cover the WebKit rule list. Additional cosmetic
rules and scriptlets run through adblock-rust. Procedural rules that cannot be
expressed as CSS and native response-body redirects are not implemented.

The benchmark build's `--youtube-check` qualifies response rewriting, pristine
iframe transport and site exceptions with fixtures. Live adblocking verification
also needs a rendered session that actually receives ads with protection off;
an ad-free visit alone does not establish that protection worked.
