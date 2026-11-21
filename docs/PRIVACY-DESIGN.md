# Browser privacy and storage

Regular tabs share the default WebKit profile. Each container has a stable,
opaque storage identity; changing its name, color or icon retains that identity.
Tabs within a private window share an ephemeral identity. Different private
windows receive different identities, and no private history or session is saved.
Explicitly downloaded or exported files remain on disk.

History uses a separate SQLite database so filtering and deletion do not rewrite
bookmarks or session state. Legacy undated history remains marked as imported.
Date filters exclude those undated entries instead of inventing visit dates.

Cookie deletion uses native live-store APIs. WPE's WebsiteDataManager currently
ignores cookies when its time span is nonzero. A date-based cookie scope therefore
means cookies for sites visited in the selected period, combined with the visible
site/search filter. It does not claim to identify a cookie's creation date.

## Browser-data import

Discovery reads profile names and known locations; parsing starts only after
Review available data. The preview is an immutable in-memory snapshot. Import
merges only the selected categories into the regular profile. Private windows
neither discover nor import source data.

The source directory is retained and individual database/JSON opens refuse
terminal symlinks and non-regular files. SQLite reads run on private copies of
the database and its WAL, with metadata/identity checks around copying and up
to three attempts when a profile changes. Source databases are never opened by
SQLite, and their journals/locks are not recovered or altered. Snapshot
folders/files are private (0700/0600) under the enzim cache and removed when
reading finishes. SQL on copies is query-only with trusted schema disabled,
a 1 MiB value limit, a small page cache and a ten-second VM deadline.

Limits per profile are 1 GiB per database/WAL file, 16 MiB per JSON input or
Firefox LZ4 output, 100,000 bookmark-tree entries with 64-level ancestry, and
64 MiB of bookmark text. History reads at most the most recent 100,000 rows or
64 MiB of text and reports truncation in the preview. Search import examines
up to 256 candidates. Unsupported/internal schemes, URL credentials, invalid dates,
unsupported templates and POST engines are excluded. Only HTTP/HTTPS pages
and self-contained HTTPS GET search templates are supported.

History batches use one transaction and deduplicate URL/timestamp identities
in the regular history scope. Existing rows and container visits are preserved;
a failed batch rolls back. Bookmark merging retains existing titles/paths for
URLs already present; new folder paths are stored as bookmark-section metadata.
Search additions retain the existing default. Normal sibling windows receive
updated bookmarks, paths and history; private windows are excluded from that
update. Failed or repeated imports can be retried without replacing existing
data. Search engines come from Chromium Web Data; its other tables are ignored and
the private database copy is removed. No password/cookie records or autofill
values are merged into enzim.

The source/profile/category flow and epoch conversion were checked against
[Brave's importer](https://github.com/brave/brave-core/blob/master/utility/importer/chrome_importer.cc),
[profile discovery](https://github.com/brave/brave-core/blob/master/common/importer/chrome_importer_utils.cc)
and [Zen's migration integration](https://github.com/zen-browser/desktop/blob/dev/src/browser/components/migration/MigrationUtils-sys-mjs.patch).

## Protection research

Brave separates filter matching by resource type and initiator, and supplies
per-site controls. Its engine implements network and cosmetic rules separately.
The browser uses those distinctions when presenting protection options.
[Brave request matching](https://github.com/brave/brave-core/blob/master/components/brave_shields/content/browser/ad_block_engine.cc)

Brave's Rust library includes conversion from supported list rules to WebKit's
content-blocking JSON. The browser uses native compiled filtering; unsupported
conversion features must be accounted for rather than presented as active.
[Brave content-blocking conversion](https://github.com/brave/adblock-rust/blob/master/src/content_blocking.rs)

Native request filtering is paired with an isolated-world cosmetic survey.
Class and ID batches, visited nodes, cached selectors and injected CSS rules are
bounded. Domain-specific hiding rules apply to containers as well as their
children, so an ad request's failure does not leave the wrapper's reserved
height. CSS-expressible actions are supported; other procedural actions are not.

Pinned uBlock source modules supply scriptlets through adblock-rust's resource
API. The source builder retains aliases, dependencies and trust requirements.
Only the pinned uAssets list receives the upstream trusted-resource bit; page
messages cannot supply script code or filters. Scriptlets execute at document
start, behind an exact-host guard. YouTube host scripts are registered for
in-page navigation as well as explicit address-bar loads. Per-origin ad choices
guard both cosmetic cleanup and page scriptlets, including after history
navigation. This does not
claim complete Brave parity or that a site cannot change its ad delivery.
Supplemental player-response rules cover queryless and query-bearing YouTube
player/next fetch and XHR requests, pruning ad configuration while preserving
content metadata and streams. Balanced mode keeps targeted first-party video-ad
endpoint rules; an origin's Allow ads choice bypasses those rules as well as the
response scriptlets. YouTube's Skip Ad controls remain visible when an ad does
reach the player.
[Brave scriptlet resources](https://github.com/brave/adblock-resources),
[uBlock source](https://github.com/gorhill/uBlock/tree/master/src/js/resources).

Brave restricts cross-origin referrers separately from same-origin navigation.
A page-side script is not a substitute for a network policy and must not be
labelled as one.
[Brave referrer handling](https://github.com/brave/brave-core/blob/master/components/brave_shields/content/browser/brave_shields_util.cc)

Ad and tracker lists are compiled separately under one exclusive native manager
lease. Each category appends its own top-origin exception rule, so allowing ads
does not cancel tracker rules. Exact scheme/host/port choices override global
defaults in both directions. A replacement is compiled fully before registration;
unchanged identities remain installed and invalid bundles leave protection intact.
Private filter compilation uses temporary session storage because compiled rules
can contain private site choices.

JavaScript, WebGL, camera/microphone/WebRTC and autoplay settings are applied to
the current document from its origin policy. Cookie acceptance and tracking
prevention apply to each native storage session. Ordinary tabs share the browser
defaults; named containers can override them for their own session, rather than
promising separate cookie acceptance for two tabs in the same session.

Mozilla's containers separate cookie stores and identify them in browser chrome.
A tab group is an organizational feature; it does not provide that storage
boundary. Container identity must survive tab moves and renderer recreation.
[Mozilla Multi-Account Containers](https://addons.mozilla.org/en-US/firefox/addon/multi-account-containers/)

Explicit navigation to loopback, private/link-local IPs, single-label hostnames
and recognized local suffixes grants the exact HTTP(S) origin to the current
native network session. Ordinary tabs retain their shared profile; containers
retain their isolated profile. Addresses entered without a scheme use HTTP for
local IPs and dotted local names; explicit HTTPS is preserved. Single words
remain search queries unless entered with a scheme or port. These session grants
are additive, so opening another local tab does not revoke an existing one.

For public names that resolve privately, Open address on the connection error
page grants that exact origin in the current profile for the current run. It
never creates a container. Grants are installed by browser navigation; webpage subresource requests do
not create grants. Historical saved container grants remain scoped to their containers;
private windows do not inherit them.

## Favicon loading

Received icons are normalized PNGs, keyed by the actual HTTP(S) source origin.
The memory and disk caches have entry and byte limits. Private windows can read
existing icons but never persist newly received images. A background completion
can update tabs at its own origin, not a replacement site's tab.

WebKit discovers and loads icons natively, including with page JavaScript disabled
or restrictive CSP. Native destination checks and content filters apply to those
requests. The SDK returns a 32×32 PNG bounded to 16 KiB with logical view,
document URI and source URI provenance. The browser rejects stale ownership and
sources disallowed by the current image policy before cache adoption. No host-side
HTTP fallback or page-script icon fetch is used.

Cache reads accept only PNGs up to 64×64 and 64 KiB; the entry count is bounded to 2048.
Normal writes use private directories and atomic replacement. A missing icon
retains the globe fallback. Completed lookups stop polling; retries and deadlines
are bounded. Cached images are reused in history, bookmarks and suggestions.

## Developer tools scope

The initial tools prioritize running JavaScript, viewing page logs, selecting an
element, reading computed styles and editing inline CSS. Chrome documents these
as Console and Elements workflows:
[Console](https://developer.chrome.com/docs/devtools/console),
[CSS inspection and editing](https://developer.chrome.com/docs/devtools/css).
The [ConsoleView source](https://chromium.googlesource.com/devtools/devtools-frontend/+/refs/heads/main/front_end/panels/console/ConsoleView.ts)
and official screenshots inform the prompt, filter/level toolbar and Keep log
layout; the browser uses native enzUI controls and its own bounded capture.
The browser evaluates through the native JavaScript API; inspection runs in an
isolated world. Page log messages are display-only input with bounded previews.
The Network view consumes the native resource log added for upstream issue 16.
It shows request lifecycles, response status, MIME, declared length and a safe
header allowlist. Query values are redacted by the engine. Native queues and
browser rows are bounded, capture stops when tools close, and navigation clears
the previous document’s capture. Response bodies and wire-level DNS/TLS timings
are unavailable.

Console preview distinguishes an unsupported native return value from a thrown
JavaScript error. It never retries an expression to recover a preview; side effects
run once. Ordinary expressions keep their top-level bindings. A leading `await`
uses an asynchronous function body and returns a bounded settled result or
rejection; declarations inside that body remain local. Console wrappers, picker
listeners, retained node references and resource capture are removed when tools
close.


## Encrypted media

The public WebKit runtime has encrypted media disabled. When a supported
runtime supplies ClearKey, the browser requires both the encrypted-media
setting and a view-level media-key-system grant. ClearKey uses view scope
because the native callback cannot attribute a requesting frame for this
permission. Camera, microphone, location, and other permissions retain
their origin checks. Licensed Widevine and PlayReady backends are unavailable.

## Opening local documents

Desktop file operands are normalized to local file URIs. PDFs use the built-in
reader. Other supported documents are streamed through a read-only loopback
handler with a random port and a 256-bit path capability. Only the selected
file's parent directory and its children are exposed; directory listings,
symlinks, parent escapes and non-regular files are refused using a pinned
directory descriptor and kernel path resolution. Responses have opaque document
origins, no CORS allowance, no referrer and no cache storage. Classic scripts,
stylesheets, images and media can load relative assets, but script fetches cannot
read sibling files through a shared origin. Module scripts and applications that
require a normal web origin should be served by a development web server.

The address bar and saved session retain file URIs rather than temporary
capability URLs. The native engine still rejects direct `file:` access. The
loopback exception applies to the local document's view; opening a file does not
create a persistent or container-wide local-network grant. The handler is
revoked when no document tab from that folder remains. Ordinary web browsing
does not start a local document handler.

## HTTP sign-in and certificate exceptions

A server's request to sign in is answered with what the person types into the
browser's own dialog, which names the host the engine reports and shows the
realm as the server's words. The answer goes to the engine for that challenge;
the browser keeps no copy, offers no saving and writes nothing to disk. The
engine reuses it for the rest of the session.

A certificate that fails validation is refused. The engine asks the browser's
policy for every such failure, and the browser answers no unless the failing
host is in a set that belongs to that one view and that only **Continue
(unsafe)** on the error page fills. The engine then accepts that certificate
for that host on the profile's network session. The set and the exception end
with the tab and the session; nothing is stored. The page is marked as not
verified in the address bar and in its site details for as long as it shows.

## Windows opened by pages, spelling

A window a page opens shares its opener's process, profile and container. It
is held before its first request until the browser has made a tab around it
with that tab's content filters, page scripts and policy; the opener's policy
has judged the address by then. While it waits it refuses a certificate that
failed validation, since nobody can be asked.

Spelling is checked on the computer by the desktop's spelling service. No
word leaves it. **Add to dictionary** writes the word to that service's
personal word list (`~/.config/enchant`), which other applications that use
the service share; a private window does not offer it. **Ignore spelling**
lasts until the browser is closed.

## Translation, page source and printing

**Translate page** is the one place where the browser itself sends a page's
address to a third party: on the person's request it opens
`translate.google.com/translate` with the address of the page on show. Nothing
is sent otherwise, and nothing of the page's content is sent by the browser.

The source of a page shown by **View page source** and the PDF of a page on
its way to a printer are written under `$XDG_RUNTIME_DIR/enzim-browser`, in
folders with mode 0700 and files with mode 0600. The source's file is named
after the page's host only. The print file is removed when `lp` returns; the
source stays until the session's runtime folder is cleared at logout.
