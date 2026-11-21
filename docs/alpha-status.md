# Alpha status

The browser and native runtime are built and versioned separately. Review the
exact source pins and runtime capabilities in [DEPENDENCIES.json](../DEPENDENCIES.json).

- The selected runtime has encrypted media disabled. Widevine and PlayReady
  are unsupported.
- HTTP downloads can resume where the server and native guard support the
  range request. Cross-site resume is still limited.
- Notification permissions may appear as default on a recreated page until
  the page requests them. Web push is unavailable.
- Passkey availability depends on the native engine and selected connector.
- Picture-in-picture is unavailable.
- Spell checking requires a desktop spelling provider and an installed
  dictionary.
- Date and time fields inside frames accept typed values.

See [browser features](features.md) and [passwords and passkeys](password-manager.md).
