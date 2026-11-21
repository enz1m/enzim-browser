# Password manager

Password management is optional and off until enabled. Choose **Browser only**
for an encrypted local vault, **System keyring** for actual credential storage
in the desktop's Secret Service provider, or add named external connections.
Session/history/settings files do not contain login passwords or TOTP seeds.

## Storage and connections

| Destination | Supported behavior | Requirements |
| --- | --- | --- |
| Browser vault | Save/update/delete, autofill, TOTP, reviewed imports, encrypted backup and CSV export | A separate vault password; desktop unlock is optional |
| System keyring | Save/update/delete, autofill, TOTP and reviewed imports; no local vault fallback | An unlocked Secret Service provider such as GNOME Keyring or oo7 |
| Bitwarden | Save/update/delete, primary website/container editing, autofill, TOTP and passkeys; other URLs and custom fields are preserved | Included Bitwarden OSS connector on Linux x86_64; account email and optional self-hosted server |

Use Password settings to add separate Personal and Business Bitwarden
connections. Each has a generated ID and its own CLI state. Bitwarden is the supported external connector. Settings from earlier builds
skip 1Password and LastPass connections.
The save dialog and login editor offer a destination picker; an update matches
only within that destination. Changing local storage does not move or duplicate
existing logins. Custom connector executable/profile paths are advanced options.
Setup guides are linked in the connection editor. The Bitwarden helper is bundled;
supported sign-in and account operations launch it internally, with no separate
CLI installation and no background helper while idle. Browser packages carry its
GPLv3 notice and pinned source archive. An advanced explicit executable override
remains available; the default never searches PATH.

The welcome screen defaults to Browser only; Not now leaves saving and autofill
off. Selecting an external manager saves its pending setup across restarts.
The address-bar password button opens that connection setup until an account
is added, then uses that account for unlock. The preferred read source is
separate from the writable save destination, so a read-only connection does
not redirect saves into an unsupported store. Additional connections remain
available in Password settings.

Bitwarden supports master-password sign-in with an optional six-digit
authenticator code. Enter the current code in the unlock form when two-step
login is enabled. The code goes to the official connector through its private
stdin prompt, never through process arguments. Master-password-only login and
subsequent vault unlock use the stdin password-file interface; authenticator
sign-in supplies the master password through an explicit child environment.

For Bitwarden's bot/authentication challenges, email verification, security-key
methods or organizational requirements, choose Personal API key in the same
form. Enter the account's `client_id`, `client_secret` and master password. API
credentials authorize the connector; the master password decrypts the vault.
The key fields and authenticator code are cleared on submission, destination
change, lock or dismissal, and are not saved in browser settings. API credentials
are supplied through the official connector's documented environment interface.
This does not implement interactive SSO or WebAuthn login to Bitwarden itself.

Connector errors distinguish verification requirements, rejected codes,
authentication challenges, account/server mismatch and rejected API credentials.
Raw connector diagnostics, which can include an echoed code, are never displayed.
Remembering a Bitwarden session stores its token
in the system keyring, scoped to profile, connection, server/account and connector
authority. Disconnecting or disabling Remember requests removal; unconfirmed
cleanup is reported. There is no plaintext token-file fallback.

Bitwarden's official local vault cache remains encrypted on disk. Unlocking or
resuming the account loads one bounded snapshot into the shared credential
worker. Matching and filling then read that snapshot without starting a CLI for
each field or login. The decrypted snapshot is wiped on lock, expiry or account
retirement. A changed encrypted cache file invalidates it; no second plaintext
or decrypted on-disk index is created.

Each browser window schedules a one-shot background initialization for its
enabled password-manager configuration after setup is complete. The shared worker restores remembered
browser/Bitwarden sessions through the desktop keyring and opens the system
store. Bitwarden warms its protected snapshot before matching begins. Initialization opens no browser
popup and installs no polling timer; idle or explicit locking does not repeat
startup restoration. It also preloads the available login summaries used by the
address-bar picker. Closing browser UI leaves this background work running;
locking the manager revokes it and clears the cached summaries.

Submitted logins are compared with saved passwords in the shared worker before
a save/update offer appears. An unchanged matching login is discarded without
rewriting its entry. Password-only forms can match an existing password within
the same site/container; new usernames and changed passwords still receive an
offer. Explicit updates recheck the stored password before writing.

Browser Settings links to a provider overview. It lists the local vault, system
keyring and connected accounts, marks the default save provider, and offers
Logins, Settings, Make default and Remove actions as applicable. Removal needs
confirmation and disconnects a provider without deleting its saved passwords.
Autofill and local vault preferences live in a separate view. Groups use headings
and dividers rather than raised panels.

Bitwarden connection settings offer inactivity locking after 1, 5, 15, 30 or 60
minutes, or locking on browser restart. Both policies keep the unlock session in
memory only and require the master password after locking or restart; signing in
to the account remains subject to Bitwarden's verification requirements. New
connections use five minutes. Existing desktop-keyring restoration remains an
explicit alternative; changing to a timed/restart policy revokes that remembered
session through the normal settings cleanup path. Other providers and local
storage retain their own lock behavior. The address-bar password popup
refreshes field detection when opened, including forms inserted after page load.
The compact picker shows borderless login rows with username and website on the
left, and Fill, Edit, Copy username and Copy password buttons on the right. A
Search input filters the list; the title row has a Manage passwords icon.
Choosing a row shows that login in the popup: a back button (to the list or
search), Edit (the full Passwords page editor), its name and site, and its
website, username and password, each with a copy button. The password is shown
as dots and read from the store only when copied; Fill login and Fill
authenticator code appear when the page has a matching field. Closing or
locking the popup forgets which login was shown. The
popup is capped at 420 pixels wide and 520 pixels high, further constrained by
the window size, and scrolls when results exceed the available space.
Search filters the unlocked accounts' available logins by name, username and
saved URL, including secondary Bitwarden URLs. Explicit selection also supports
Bitwarden logins with no website or a legacy HTTP website; they are not automatic
website matches. A login found by search that was saved for another website
than the page is not filled at once: the popup names both websites, and Fill
has to be chosen a second time. Editing opens the full Passwords page. After a sign-in, the save offer
compares the submitted password with the logins saved for the page and with the
login the browser filled into that form, so a login chosen by search for
another address is not offered again as a new one. Copy reads require a live
provider grant and the framework’s sensitive clipboard API. Protected text is
offered for 30 seconds with a password-manager history-exclusion hint, without
an ordinary text action, keeper handoff or helper process. Expiry revokes only
that source; replacing the clipboard is safe. Desktop history managers can
retain their own copies and may ignore the advisory hint.

Editing a Bitwarden login writes only the fields that were changed here,
compared with the login as it was shown: a note, a name or an authenticator
seed that another device saved in the meantime stays. When the account cannot
be reached to check for newer changes, nothing is written and the account
stays unlocked. The decrypted snapshot holds logins only; cards, identities
and notes are dropped when it is taken. A saved website without a scheme
(`example.com`) is read as its HTTPS site, and a new login is saved under the
page's address without its query and fragment. Removing a connection signs
its connector profile out and deletes the profile this browser created for
it; a profile directory you chose is left in place.

Use the refresh button beside Search logins after making changes on another
Bitwarden client. There is no recurring network polling. Existing-item edits
synchronize first, with a bounded timeout, so newer unmanaged fields and
passkeys are not replaced by an older cached record. Editing the primary
website preserves other URLs and their match rules; if that URL has changed
elsewhere, the edit requires a refresh. A failed refresh leaves cached unlocked
logins readable; a failed pre-edit sync prevents the write. Saving and deleting
invalidate the snapshot so subsequent matches reflect the mutation.

The integration is a native CLI connector; browser extensions are not enabled
by it.

System storage keeps a versioned credential document as a Secret Service
secret. A non-secret revision file revokes other clients before a mutation;
login data is never stored in that marker. A denied/unavailable keyring fails
closed. Browser-vault backups, CSV exports and rekeying remain browser-vault
operations requiring its password. Use a connected manager's own import/export
tools for external vault migration.

Windows in one browser profile share a credential worker and serialize connector
operations. Opening another window reuses an already unlocked destination.
Each autofill/export reply retains the authorization of the exact store read;
a request in another window cannot substitute its store's authorization.
Locking the shared manager revokes pending replies across those windows.
A manual lock also suppresses automatic field-focus keyring reopening until an
explicit password-manager action unlocks it. Closing one manager view clears
its inputs while work already authorized in another window continues.

Native helpers run outside the UI thread with explicit environments, bounded
output, cancellation and a whole-request deadline. Passwords and JSON payloads
do not enter command arguments. Request/output buffers use wipe-on-drop custody;
OS, runtime and helper-internal copies remain outside that guarantee. Connection
changes revoke fill leases, cancel active helpers and reject stale unlock forms.
Stale settings pages cannot silently restore a disconnected account.

## Browser vault and locking

The vault uses Argon2id v19 (64 MiB, three passes, one lane) to derive a 256-bit
key from a separate vault password and a random 128-bit salt. XChaCha20-Poly1305
authenticates the complete versioned header and encrypted document, with a fresh
192-bit nonce on every publication. The implementation uses RustCrypto crates;
it does not implement cryptographic primitives itself. The vault password cannot
be recovered. Use a unique password and keep an encrypted backup.

With **Remember browser-vault unlock** enabled, the built-in provider remembers
its derived unlock key through the framework's Secret Service store. It uses
the active desktop provider, including GNOME Keyring or oo7, without selecting
or replacing daemons by package name. The vault password is never stored there.
An existing vault is enrolled after one successful password unlock; a newly
created vault is enrolled immediately when the desktop provider accepts it.
If no provider is available, access is denied or an unlock prompt is dismissed,
the encrypted vault keeps its password-unlock path.

The keyring entry is scoped to this application's vault path and contains a
versioned key packet bound to the vault's salt. Retrieval still authenticates
the encrypted file and validates its contents; an obsolete or substituted key
cannot bypass that check. Changing the vault password refreshes the remembered
key. An unavailable keyring during that update leaves password recovery usable.
Exports, backups and password changes retain password reauthentication.

Startup, a login-field focus or opening the password manager can request desktop unlock.
Unchanged field reports do not repeat it. Explicit Lock suppresses automatic
reopening until an explicit unlock action or a desktop-session transition.
Inactivity clears the local decrypted vault; a later login interaction can
unlock it through the desktop again. No keyring polling timer is installed.
Turning the option off requests deletion of its remembered entry; an unconfirmed
removal is reported with a retry action. **Forget** removes the entry separately;
a later password unlock can remember it again while the option remains enabled.

Vault files are private, owner-only files in an owner-only credentials folder.
Publication uses the framework's inode and parent-directory custody checks,
atomic replacement and durable write path. Symlinks, multiply linked files,
public permissions and stale writers are refused. Another window's publication
invalidates an unlocked copy and any previously issued fill authorization.

The browser-vault worker owns the derived key. Locking drops it and the decrypted
records, revokes pending grants and clears GUI drafts. Idle locking is
configurable; desktop session locking also revokes access. Expensive key
derivation and vault/import/export I/O run outside the UI thread. The locked
worker blocks without polling. An unlocked worker also sleeps until a request,
a native directory-change notification or its exact inactivity deadline.
File watches share the framework's native backend and retain the same private
file-custody checks; changes from another window revoke stale keys. Browser UI
receives lock notifications rather than polling the worker. Editing a login
refreshes its open destination's inactivity deadline through throttled activity
events, without a periodic keepalive or reviving an expired destination. Save offers and
exports use single expiry deadlines. A one-second countdown runs only while a
valid authenticator code is visible in the manager editor; ordinary pages and
logins without an authenticator have no TOTP timer. With the manager disabled,
pages receive no credential script.

Secret-bearing messages, records and native evaluation results redact Debug
output. Opening browser forms revokes webpage keyboard/IME ownership and
pending paste receipts; delayed replies cannot paste browser input into a page. Secret buffers are wiped when their last owner drops, and temporary
serialization/decryption buffers use zeroizing ownership. Credential input
makes the browser non-dumpable on Linux before accepting a master password.
This is defense in depth, not a guarantee that all copies in renderer,
allocator, OS, GPU or swap memory are erased. A compromised browser, operating
system or unlocked desktop remains outside the protection of vault encryption.
The implementation has not received an independent security audit.

## Filling and saving

Clicking a visible username, email, password or authenticator field offers a
native dropdown only after an unlocked, usable matching login is confirmed.
Pending lookups, empty results and locked accounts do not paint inline popups.
A locked vault can try its enabled desktop unlock once; the address-bar key
button and Ctrl+Shift+L remain available for manual unlock. Secret inputs are
replaced by a native progress indicator while connecting.

Filling happens only after choosing a login. The selected field identity is
retained across native popup focus, and focus returns to that field without
submitting the form. Delayed matching results cannot populate another field's
popup. Inline suggestions require the exact HTTPS origin, including port, and
container. A matching subdomain alone is insufficient. The current page must
have a valid TLS certificate and be visible and fully loaded.

The address-bar search also permits deliberate selection of a login whose saved
website does not match. This explicit action can override website matching, but
not container, browser-profile, current-document, TLS or provider authorization
checks. The credential is delivered only to the selected field on the current
secure page; its saved website is not rewritten. Copying is also a deliberate
item selection and does not require a focused website form.

Within that origin boundary, Bitwarden Exact and Starts with rules use the
current full page URL. Never, unknown and regular-expression rules are not
offered as automatic matches; their stored data is preserved. This is narrower than
Bitwarden's cross-subdomain matching policy.

The isolated-world form bridge has no access to the vault. A short-lived native
authorization binds the selected login to the active tab, document nonce and
selected field identity. It rejects hidden, read-only, disabled, replaced,
nonempty or cross-origin-action password fields. Frames do not receive
credentials. Username-only login steps receive only the selected username;
the password is not included in their fill operation. Ordinary search/text
fields do not trigger username-only suggestions. The page receives only the chosen credential when filling; the
bridge never submits the form. TOTP-only filling does not release a password.
A website can read a password once it has been filled into that website's form,
as with other password managers.

Embedded login forms are not yet captured for saving either. For example,
Plex’s `www.plex.tv/sign-in/` embeds its login from `app.plex.tv`, so signing
in there currently produces no save offer. Supporting this requires native
frame provenance and frame-targeted protected evaluation, tracked in
WPE issue48.
A manually added login will not enable filling that embedded form.

Save offers follow a trusted form interaction and require explicit confirmation.
Saving is disabled in private windows. Access to existing logins in private
windows is separately opt-in. The full management page is available only in a
regular window.

## Migration and authenticators

Imports accept Chrome/Firefox-style password CSV, unencrypted Bitwarden login
JSON, and enzim encrypted backups. Imports are bounded, reviewed before saving,
validated before publication, and merged without duplicating identical logins.
HTTPS website logins are supported. Non-login Bitwarden items and encrypted
Bitwarden exports are not imported. Exports containing passkeys are explicitly
refused while native passkey migration is unavailable; keys are not silently
discarded. Original browser profiles are never read for
password decryption.

Encrypted backups use a separate backup password. CSV export is unencrypted and
requires current-vault-password confirmation plus an explicit plaintext warning.
Exports use private files and cannot overwrite the active vault. Delete migration
files securely according to the storage device's capabilities after verifying
the migration; the browser does not silently delete user-selected files.

TOTP accepts Base32 secrets, with or without spaces, hyphens, padding and
lower case, and `otpauth://totp` URIs; parameters other than the secret,
algorithm, digits and period are ignored. SHA-1, SHA-256 and SHA-512, six to
eight digits and custom periods are supported. HOTP and Steam codes are not.
Codes are computed in the browser for every provider, from the seed saved
with the login. The login editor shows the current code with its remaining
seconds and copies it through the sensitive clipboard; the popup and the
inline dropdown fill it into a field marked `one-time-code`, or one whose
name or label says it takes a verification code. A Bitwarden item whose
authenticator field this browser cannot read stays usable without codes, and
its field is left as it is.
TOTP seeds are encrypted alongside passwords. Keeping both factors in one vault
has different isolation properties from a separate hardware authenticator.

## Provider replacement and passkeys

The UI and form bridge use a worker-owned `CredentialProvider` interface.
Storage and authentication belong to the selected native provider; callers do
not receive its key or storage object. Native destinations share one router
behind the credential-provider contract. A future replacement must retain the
same authorization, cancellation and origin/frame checks, and revoke the old
provider before handover. This does not enable browser extensions.

The browser provides [passkeys](#passkeys) through an isolated-world handler.
The browser validates the calling frame origin and relying-party ID, builds
client data, and asks the user to confirm outside the page. The selected WPE
engine has no native WebAuthn authenticator backend.

What an engine-owned implementation would still add: the ancestor check for
a same-site frame nested inside another site's frame (only the frame and the
top document are compared), transient user activation, native
interface objects that a page cannot redefine for itself, and conditional
mediation in the engine's own autofill. A pending request is cancelled when
the page starts another load, the tab changes or the page aborts it.

## Design references

- [Firefox LoginManagerParent](https://github.com/mozilla-firefox/firefox/blob/main/toolkit/components/passwordmgr/LoginManagerParent.sys.mjs): origin and browsing-context checks, explicit save policy.
- [Firefox crypto-SDR](https://github.com/mozilla-firefox/firefox/blob/main/toolkit/components/passwordmgr/crypto-SDR.sys.mjs): decoder-ring storage and primary-password behavior.
- [Chromium PasswordManager](https://github.com/chromium/chromium/blob/main/components/password_manager/core/browser/password_manager.cc): native committed-origin and saving/filling eligibility gates.
- [Bitwarden autofill design](https://github.com/bitwarden/clients/blob/main/apps/browser/src/autofill/autofill.design.md): active-tab verification and separate submission decisions.
- [Bitwarden FIDO2 UI](https://github.com/bitwarden/clients/blob/main/apps/browser/src/autofill/fido2/services/browser-fido2-user-interface.service.ts): request sessions, explicit selection/verification and aborts.
- [WebAuthn](https://www.w3.org/TR/webauthn-3/) and [RFC 6238](https://www.rfc-editor.org/rfc/rfc6238).
- [Secret Service specification](https://specifications.freedesktop.org/secret-service/latest-single/) and [oo7](https://github.com/linux-credentials/oo7): desktop-owned unlock and credential storage.

The browser implementation is original; these projects were read for security
and interaction design, not copied into this repository.

Explicit username/password copying uses the framework sensitive clipboard path
from issue 3684.
Ownership or transport refusal is reported without falling back to ordinary
clipboard text. TOTP remains available through its explicit Fill code action.


CSV/Bitwarden imports use the current tab's session/container. Encrypted backups
retain their stored container identifiers. A container absent from this browser
is shown as unavailable; explicitly choose an existing container or the default
session in that login's editor before filling it there. CSV migration cannot
preserve another browser's container identities.

Verification results are recorded in the archived maintainer qualification records. The original verification covered 142 application checks, 16 native browser assertions and two native secret-script checks, with deliberate fault controls. Desktop-unlock verification adds the current credential regression run, isolated real GNOME Keyring and oo7 round trips, and actual rendered settings/unlock flows.

The dropdown uses the framework's anchored overlay and autohiding scroll area.
Its wrapper remains mounted while closed so page keyboard focus survives popup
changes. Field coordinates are scaled/clipped to the current native viewport;
typing, scrolling, navigation and stale selection invalidate old suggestions.
No field-position polling or additional repeating timer is installed.

Ordinary Tab presses are forwarded directly to WebKit. Controlled test-page
focus-title markers are not used as a production prerequisite. Authoritative
DOM-to-toolkit boundary handoff remains tracked in
WPE issue47.

## Passkeys

Websites can create and use passkeys (Web Authentication) while a password
manager is enabled. A passkey is kept with a saved login: in the browser
vault, in the system keyring, or in a Bitwarden account, where it has the
form Bitwarden's own clients write and read, so one made here signs in from
Bitwarden's apps and one made there signs in here.

The engine is built without Web Authentication, so the browser provides it.
A script in the page's world defines `navigator.credentials` and
`PublicKeyCredential` and relays each call through an isolated world to the
browser. Nothing the page says is trusted:

- The calling origin is the one the engine recorded for the frame. Pages
  without HTTPS, pages with a certificate error and frames inside another
  site's page are refused.
- The relying party ID must be the page's host or a registrable domain above
  it, checked with the public suffix list. Anything else is a `SecurityError`.
- The browser writes the client data (type, challenge, origin) itself.
- Every request opens the browser's own prompt. Creating a passkey shows the
  account and where it will be saved; signing in lists the saved passkeys for
  that website and container, and nothing is signed until one is chosen. A
  locked store is unlocked in the same prompt. Closing the prompt answers
  `NotAllowedError`, the same answer a website gets when nothing is saved.
- The private key leaves its store only to sign: it is never given to the
  page.

Credentials are ES256 with `none` attestation, discoverable, and report user
verification: the store was unlocked and the request confirmed in the prompt.
One saved in Bitwarden reports itself as synchronized, one in the vault or
keyring as bound to this device. A login holds one passkey; a second one for
the same login is refused instead of replacing the first, and a relying party
that lists a saved credential as already registered gets `InvalidStateError`.
Private windows can sign in when private autofill is on and never create.

Not provided: passkeys in the autofill dropdown (conditional mediation
reports unavailable), security keys and phones as authenticators, algorithms
other than ES256, attestation, and passkeys for frames embedded in another
site. The CSV export does not contain passkeys; the encrypted backup does.

`tools/check_passkeys.py` runs a registration and two sign-ins through the
real engine, with the page verifying each answer in WebCrypto.

## Known limits

- Bitwarden sign-in with an authenticator code passes the master password to
  the connector in its environment for that one call; the other paths use
  stdin. The account email and server address are connector arguments.
- The browser does not run the connector's own `lock`: locking wipes the
  session and snapshot held here, and the connector profile stays signed in
  until the connection is removed.
- An error from a provider still locks that provider, except an edit that
  could not reach Bitwarden.
- A Bitwarden website without its own match rule is matched by exact origin,
  whatever the account's default rule is.
- An import stops at the first row it cannot take (a website without HTTPS, an
  unreadable authenticator secret) instead of skipping it.
- The CSV export writes values as they are: a username or note that begins
  with `=`, `+`, `-` or `@` is a formula to a spreadsheet. It does not contain
  passkeys.
- The Secret Service session used for the system keyring is unencrypted on
  the session bus (framework).

## Connector references and verification

- [Bitwarden CLI](https://bitwarden.com/help/cli/) and its [open-source clients](https://github.com/bitwarden/clients/tree/main/apps/cli).

Real isolated GNOME Keyring and oo7 store/read/delete tests pass. Official
Bitwarden CLI2026.10.1 is exercised against two isolated Vaultwarden1.37.5
accounts, including preservation of synthetic native passkey fields, and a
passkey created here that is read back whole, signs a verifiable assertion and
survives a later password edit
(`real_bitwarden_keeps_a_passkey_made_here_and_signs_with_it`).
