# Browser contributor instructions

- Read README.md, docs/development.md and CONTRIBUTING.md before changing code.
- Keep fixes for shared UI or engine behavior in their dependency repositories.
  Update browser pins after those changes are integrated and verified.
- Preserve profile isolation, credential handling, native sandboxing and source-pin checks.
- Build and test in the foreground, with one compiler job, a 3 GiB memory ceiling
  and zero swap. Respect CARGO_TARGET_DIR and reuse existing build artifacts.
- Filter Rust test runs. Verify new behavior checks against a deliberate fault.
- Verify UI changes in both themes and at small sizes, using disposable profiles.
- Keep comments focused on contracts, invariants and non-obvious constraints.
  Give every lint suppression a reason and every unsafe block a safety contract.
- Keep profiles, credentials, downloaded programs, raw captures and native
  runtime binaries outside Git. Add documentation screenshots under
  `docs/screenshots/`.
- Stage explicit paths. Do not rewrite history or publish releases as part of cleanup.
