# Contributing

Read the [development guide](docs/development.md) and the relevant feature or
privacy documentation before changing browser behavior.

Keep shared UI fixes in enzUI and native engine fixes in the WebKit SDK. Review
and verify dependency changes before updating the browser pins. Do not copy
framework internals into the application.

Use disposable profiles for tests and visual checks. Run focused Rust tests,
keep native sandboxing and source verification enabled, and verify new behavior
checks against a deliberate fault. UI changes need both themes and small-window
coverage. Performance claims need comparable workloads, independent repeats,
full-load measurements and separate process/GPU memory accounting.

Respect `CARGO_TARGET_DIR`; reuse existing artifacts and avoid `cargo clean`.
The helper scripts use one compiler job, a 3 GiB ceiling and zero swap.

Keep comments about contracts, invariants and non-obvious constraints. Each lint
suppression needs a reason; each unsafe block needs a safety contract.

Include the problem, resulting behavior, focused validation and remaining limits
in change descriptions. Keep unrelated formatting and dependency updates separate.
