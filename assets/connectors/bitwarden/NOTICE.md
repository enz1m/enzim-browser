# Bitwarden connector

The bundled helper is the unmodified Bitwarden OSS CLI 2026.10.1 for Linux x86_64,
executed as a separate process. It is licensed under GPLv3; the browser's own
source remains MIT. The Bitwarden name identifies the service and component;
this project is not affiliated with or endorsed by Bitwarden. No logos are bundled.

Source and license: https://github.com/bitwarden/clients/tree/cli-v2026.10.1

The package preparation tool verifies the official release archive and executable
against the pinned SHA256 identities. It places the original source archive and
license notices beside the helper. Keep these files together when packaging.
