# Browser features

New website tabs show a small, theme-aware loading pulse until their first page
frame arrives. Its frame clock is capped at 20 Hz, honors reduced motion, and
is removed after paint or cancellation.

Visible websites scroll under the pointer while another window is active,
without taking keyboard or text-input focus.

Tabs can appear across the top or in either sidebar, with text or icons.
The top strip uses the hover-close area for titles and switches to favicons
only when a title no longer fits; extreme counts use
a menu that keeps the selected tab visible without a scrollbar. Top-bar groups
use compact color markers; hover reveals the name and a grouped tab's context
menu opens its settings. The close button appears over the title in a small
circle inset from the upper-right corner on hover. Middle-clicking an empty part of the top strip opens New Tab;
middle-clicking a tab closes it.
In the sidebar and the icon rail the pinned tabs stay at the top while the
other tabs scroll under them, with no scrollbar; a tab still drags between
the two at any scroll position.
A tab that opens slides in and fades in; when a tab opens or closes, the tabs
after it slide to their new places rather than jumping (both snap where the
desktop asks for less motion).
Standalone tabs and colored group blocks share a persistent order; drag tabs
into groups or between them, or move a whole group by its header.
Dropping onto the middle of another tab creates a group or joins that tab's
existing group. The outer quarters remain insertion zones for reordering.
Empty sidebar space accepts a drop at the end, with an insertion marker rather
than a pane-wide highlight. Sidebar context menus offer **New group**; tab menus
show **Remove from group** only for group members and nest **Open in container**.
A group's own menu and its settings dialog offer **Ungroup**, which keeps its
tabs where the group stood, and **Close group**, which closes them with it.
The sidebar context menu groups tab placement under **Tabs location**. Its
**Containers** submenu opens a tab in a selected container, with **Manage
containers** below a divider. The browser menu offers the same container choices.
Pinned tabs sit above a divider at the top of the sidebar, without a heading.
**Immersive view** (browser menu, Ctrl+Shift+E) gives the page the whole
window inside a thin frame. There is no top bar: the sidebar carries what it
held. Above the tabs are back, forward and reload with the window controls
(a row the window is dragged by; the sidebar stays through the move), the
address field with the bookmark star, and the site's tools (copy URL,
passwords, privacy, site permissions) with a new-tab button; at its foot, under a divider, the
library buttons (bookmarks, history, containers, Appearance) with downloads
and the menu. Clicks and scrolls on the sidebar never reach the page under
it. The sidebar
waits outside the window and slides in over the page when the pointer rests
within 36 px of its side for 0.175 s, so
a pointer passing by does not call it. It stays while the pointer is over it
and goes when the pointer moves over the page or leaves the window. It also
comes in for Ctrl+L, and stays while something opened from it is open: the
address suggestions, the menu, a popup such as Appearance, or a context
menu. Its glass takes the colour scheme's hue, and the address field lets
some of the glass through. While immersive view is on, Appearance →
**Immersive sidebar** picks what the sidebar is made of: Glass (clear, bending
the page across its whole face), Frost (the default), Mist (a
light frost), Blur (flat, no rim) or Solid (opaque, field included). The
choices are kept with the other settings.
Bookmarks, downloads and a built-in
PDF reader are included. Settings, bookmarks, dated history and
containers open as browser pages. Settings → Website data → Cookies and site
data opens a dialog: the cookies grouped by site with a count, one site opened
to its cookies, a search field, the container and a visited-in period, removal
of one cookie, one site's or all shown, and clearing all of a container's
website data. Each removal asks first and returns to the dialog. The sidebar contracts as the window shrinks;
the minimum window size is 420 × 360. Themes follow the framework theme file; native website color-scheme preferences
follow the effective theme too. The palette button beside the library actions
opens [Appearance](appearance.md): mode, colour theme, a solid, grain or glass window
frame, corners, and the sizes of the top bar, the URL bar in it, sidebar, icon rail
and tabs, kept in the browser's own theme file. Sidebar library actions and New Tab live at the
top; the icon rail keeps library actions at the bottom. Internal pages use
distinct native icons. The browser menu groups zoom controls in one row; a
non-default zoom indicator inside the address bar opens 25% presets and reset.

Normal windows created from the browser share native storage and preferences.
Each private window has separate ephemeral profiles and does not save browsing
history or its session. Its temporary preference changes remain private.
Downloaded and explicitly exported files remain on disk. Named containers keep
website storage separate.
Containers have a color and one of twelve selectable icons. The icon-only tab
rail uses a slim color line beside the favicon; its tooltip names the container.
Group backgrounds extend slightly wider while keeping grouped icon tabs the
same width as ordinary icon tabs.
Click a container's name on its library page to open an empty tab in it. The new-tab
dialog's lower-left icon opens the container menu. Suggestions use cached site
names and favicons and match history and bookmarks as you type; Up/Down selects
a result and Enter opens it. The dialog starts above the window center and keeps
its top edge fixed as suggestion results grow or shrink downward. Host matches include a home-page shortcut before
individual visited pages.

Bookmarks, History and Containers open as library views with active sidebar
buttons. They preserve the selected webpage and its form/scroll state without
creating tabs; select its tab or press Back to return. Localhost, private network
addresses and local hostnames work in ordinary tabs. A `file://` address, or
the path of an existing file (`/home/…` or `~/…`), typed into the address bar
opens that file. Typed local IPs and dotted
local names use HTTP by default; an explicit HTTPS scheme is preserved. An
unrecognized local destination can be opened in the same tab using Open address
on its connection error page. No container is required.

Typing in the address bar, empty page or new-tab dialog uses the same ranked
history and bookmark suggestions, including cached site names and icons.
Up/Down chooses a suggestion; Enter opens it in the current tab from the address
bar or empty page, and a new tab from the dialog. Middle-clicking empty sidebar
space opens that dialog; middle-clicking a tab closes it.

Browser pages, popovers, sidebar lists, PDFs and developer panels use the
framework's thin autohiding overlay scrollbar, including its motion and theme
settings. Download actions retain clearance from the scrollbar's grab area.
Download rows show the filename, size and date, with file/folder actions inside
the same hover area. New download dates are retained across restarts.

Settings → Browsing → On startup chooses between restoring open tabs, an empty
new tab, or a specific website. Restoring tabs is the default for existing
profiles. Private windows always start fresh and do not save their session.

**Import browser data** is available in Settings and the browser menu. Choose a
browser/profile, review the available data, select categories and import.
Automatic discovery covers Firefox, Chrome, Chromium, Zen, Brave, Edge, Vivaldi,
Opera, LibreWolf, Waterfox and Floorp, including common Flatpak/Snap locations.
Choose folder accepts another compatible profile or its profile-root directory.

Import supports HTTP/HTTPS bookmarks and dated history, plus compatible HTTPS
GET search engines. It adds to the regular profile, skips duplicate URLs/visits
and preserves existing bookmark titles and the default search engine. Imported
bookmark folder paths appear as searchable sections. History dates retain their
original meaning after Firefox/Chromium timestamp conversion. Review again
refreshes a profile's preview; no data is added to the browser profile until
Import is pressed.

Private windows do not import data. Passwords, cookies, open tabs, autofill and
other browser settings are not imported. Existing enzim favicon
cache entries are used; new origins receive icons when visited. Source profiles
are read without writable handles; database copies include the WAL, and a
changing profile can require closing its browser before retrying. Size bounds
and source-snapshot handling are described in
[privacy and storage](PRIVACY-DESIGN.md#browser-data-import).

**Passwords** in Settings or the browser menu opens the optional credential manager.
Choose an encrypted browser vault, actual System keyring storage, or named
connections to Bitwarden. Several Bitwarden accounts can be connected and
selected as save destinations, with saves, updates and autofill; its connector
is included. Websites can create and use passkeys, kept with your saved
logins. See [password manager](password-manager.md) for setup, security and
verification limits. The address-bar key and Ctrl+Shift+L remain available;
clicking a login field offers exact HTTPS-origin/container matches.

Search supports DuckDuckGo, Google, Startpage, Brave Search, Bing, Wikipedia and
custom HTTPS templates. History supports time ranges, literal text filtering,
individual deletion and confirmation before clearing matches. Cookie management
uses native inventory and exact name/domain/path deletion without exposing
cookie values. Cookie date ranges select **sites visited during that period**;
WebKit does not expose cookie creation dates.

Linux “Open with” integration accepts multiple documents in one launch: HTML,
XHTML, XML/feeds, JSON, plain text, PDF, common web images, and supported
audio/video. PDFs use the built-in reader. Saved pages can load assets in their
folder and child folders; symlinks and parent-folder assets are refused. Local
documents use opaque origins, so web applications that need fetch, modules or
origin storage should use a development server. Background operands load when
selected. See [desktop installation](development.md#desktop-entry-for-a-source-checkout).

Page exports include HTML, MHTML, PDF and full-document PNG. HTML saves the
original loaded source; linked resources are not included. MHTML embeds the
resources WebKit archives. PNG exports include off-screen content within the
native image dimension and memory limits. Oversized exports report an error.
PDF preview keeps a bounded in-memory page cache and prepares the next page.
The toolbar can open the original document in the default system PDF app.
PDF reading requires Poppler's `pdfinfo` and `pdftoppm`; PDF printing uses CUPS
`lp`. These programs must be installed. No physical printer is required to save
PDF files.

Builds prepare the bundled Balanced and Strict native rule caches with the pinned
WPE runtime. Fresh profiles load those entries before navigation. Custom site
policies and incompatible or damaged cache entries retain native compilation
before a page can load. Native builds therefore require the prepared runtime;
cross builds retain compilation on the target.

Unchanged immutable rule sources reuse their native cache identities through a
bounded weak-reference cache, avoiding repeated checksums on navigation. Changed
sources still use the native identity and filter installation checks.

Privacy protection combines native WebKit network filters with cosmetic
ad-container cleanup and document-start scriptlets. Bundled EasyList,
EasyPrivacy and uBlock filters use Brave's `adblock` engine. YouTube player
scriptlets remove ad data while retaining the video response, including
queryless player requests. Targeted video-ad endpoints are blocked in Balanced
mode too. Balanced and strict modes and independent per-site ad and tracker exceptions are available;
allowing ads preserves tracker and storage protections. Global defaults live in
Settings; the Privacy popup adjusts the current origin, including its port.
Toolbar popups use compact icon/title headers, consistent spacing and bounded
scrolling. Escape, clicking the trigger again or clicking outside dismisses a
popover. Dialogs retain explicit confirmation/cancellation actions; Find uses a
Done button. Privacy and permission controls retain their existing scope and
authorization behavior.
Some procedural and
response-replacement rules remain unsupported. See
[privacy design and sources](PRIVACY-DESIGN.md) and
[third-party notices](../THIRD_PARTY.md). Privacy controls include tracking
prevention, third-party cookies, URL parameter cleaning, JavaScript, WebGL,
WebRTC, autoplay and user-agent overrides. Camera/microphone APIs are enabled
by default; each site must still request device access. Video and voice calls
use WebKit's LibWebRTC (`RTCPeerConnection`, data channels, VP8, VP9 and H.264
video, Opus and G.722 audio, and `RTCRtpScriptTransform` for end-to-end
encrypted calls); their sockets are opened by the network process, so web
processes keep their sandbox without network. Screen sharing
(`getDisplayMedia`) asks each time ("Let this site see your screen?", never
remembered); the desktop's own dialog (the ScreenCast portal, GNOME's "enzim
Browser wants to share your screen") chooses the screen or window, and
cancelling it gives the page `NotAllowedError`. A page granted the microphone
lists the speakers and plays to the one it picks (`setSinkId`), also while
playing. Until a page is granted the camera or
microphone, its local addresses are announced as mDNS names, which needs Avahi
on the desktop for calls inside one network; a site's STUN or TURN server
connects them otherwise. `tools/check_web_rtc.py` places a call between two
tabs. Webpage copy publishes
the user-triggered selection to the desktop clipboard. Ctrl+V and context Paste
read desktop text asynchronously; delayed replies cannot follow a tab, document
or focus change.

Websites can request desktop notifications through a native permission prompt.
Allow/Block choices are scoped to the requesting top-level origin and container.
Settings offers **Never allow** for a global block; private windows never prompt
or post notifications. Native desktop entries relay click/dismissal back to the
page. This covers live-page notifications, not background Web Push. On the
selected WPE runtime, a recreated page initially reports permission as default
until it calls `requestPermission()`; that call applies the saved choice without
another prompt. Scoped native permission-cache initialization is tracked in
WPE #41.

For YouTube’s standard HTTPS origins, the default Linux/WebKit identity uses
a Safari-compatible platform token for compatibility. YouTube can serve ES5
or ES2021 scripts in modular or monolithic form; the identity alone does not
guarantee matching bundle bytes. Engine/version tokens and explicit user-agent overrides are preserved.
Other sites retain the native default identity. The upstream platform-identity
issue is tracked in WPE #82.

The user-agent selector shares OS, browser and version choices between global
privacy settings and containers. A container can inherit the global choice or
set its own. Custom strings remain available. Changing a user-agent does not
change the rendering engine.

New profiles start with a native welcome flow for search, privacy and password
choices. Finishing opens `https://browser.enzim.dev/`; password setup can be
continued later from Passwords. The compact welcome keeps its actions visible
while content scrolls. Search-provider choices use text labels without logos or
letter badges. Existing profiles keep their setup. `-P` opens the profile chooser;
Startup restores the last selected profile; an explicit profile argument or `-P`
overrides it. `--create-profile NAME` creates and opens a fresh profile, and `--profile NAME`
(or `-P NAME`) selects one. Named profiles isolate browser settings, native
website storage, history, caches and password stores. A running profile receives
its later URL/private-window launches through its own instance channel. URLs
can follow a named profile directly, for example `-P Work https://example.org`.
A launch opens a default-container tab and preserves restored tabs.

Containers can use a proxy at a numeric address: an HTTP CONNECT proxy, such as
`127.0.0.1:8080` or `[::1]:8080`, or a SOCKS5 proxy, such as
`socks5://127.0.0.1:1080`. A proxy that asks for a username and password gets
them, as HTTP Basic credentials or SOCKS5 username/password; the username is
kept with the container's settings and the password in the desktop keyring
(Secret Service), never in the browser's files, and is read once a session.
The destination guard stays active; the proxy is only ever asked for a
numeric destination, and a refusal, including a rejected login, fails without
a direct fallback. DNS resolves locally. Changing an existing container’s
route or username requires a browser restart. WebRTC is disabled for proxied
containers because its socket routing is not qualified. Hostname proxy
endpoints and SOCKS5 with remote DNS are unsupported.

Each container can inherit or override the browser user agent and individual
privacy defaults. Cookie and tracking-prevention policies apply to its native
storage session; per-site choices stay within that container. Per-site controls cover ads, trackers, JavaScript,
WebGL, camera/microphone/WebRTC, autoplay and URL cleaning.
Turning off **Allow agents** in a container's settings hides its tabs from
the assistant and from agents connected through the agent center and refuses
their calls on it, including opening pages in it; see [agents](agents.md).

The assistant is a chat beside the page in the window's frame, with an
OpenAI-compatible service or a ChatGPT sign-in; see [assistant](assistant.md).

Favicons are cached by HTTP(S) origin under
`$XDG_CACHE_HOME/enzim-browser/favicons` and reused in tabs, history, bookmarks
and new-tab shortcuts after restart. Private windows keep newly received icons
in memory and do not write them into that cache. WebKit discovers and decodes
icons natively, including cross-origin icons and pages with JavaScript disabled. Native content filters and destination
restrictions apply before requests; the browser validates document ownership
before adopting bounded PNG results.

Developer tools include a JavaScript console, element picking, CSS selectors,
parent/child DOM navigation, computed styles and live inline CSS editing. The
resizable dock includes console levels, text filtering, command history and
Keep log. Prefix an expression with `await` to inspect its settled value or
rejection; expressions execute once. The Network view filters native requests
by text, type and errors and shows status, request duration, MIME, declared
lengths and allowlisted headers. Bodies and DNS/TLS phase timings are
unavailable. Console capture, DOM picking and resource logging run only while
tools are open. Ctrl+F opens page search below the document.

The selected native runtime is WebKit v0.1.1. Its source and runtime
archive are pinned in [DEPENDENCIES.json](../DEPENDENCIES.json). Hidden tabs
release the browser-held GPU frame and the configured memory policy schedules
background cleanup.

Closed-page renderers exit instead of staying in the process cache. Hidden pages
release rebuildable caches after a minute, with maintenance every two minutes
while hidden; loading and audio defer this.
Release also drops hidden compositor tiles and returns their driver buffers; silent
video decoders, including streaming video, suspend while hidden and resume at
the saved position on show. Off-screen muted videos also release their decoder,
and each renderer shares one VA display. The next full-page scroll may
repaint discarded tiles. The page remains loaded with its state. The back/forward
cache keeps two pages for up to ten minutes. See
[tab memory behavior](tab-memory.md) for scheduling and settings.

A site's request for the camera, microphone, location or notifications opens
a prompt. Block is remembered for the camera, microphone and location as for
notifications, and shows as Block in the site's permissions; Allow once is not
remembered. A request blocked or dismissed (Escape, clicking away) is refused
without a prompt for the rest of the session, also for kinds that are never
remembered, such as screen sharing. Changing or resetting a site's permissions
lets it ask again.

Encrypted media is off by default. Supported runtimes expose a ClearKey switch
in Settings, site Privacy and container overrides. The API setting and explicit
view policy grant are both required; device permissions remain origin scoped.
ClearKey permission currently covers the whole view rather than providing
frame-specific prompts. Widevine and PlayReady are unsupported; importing a
Firefox or Chrome CDM does not supply a compatible WPE backend.

A page's `alert`, `confirm` or `prompt`, a list's choices and a server's
request to sign in belong to the tab that raised them: they are on show only
while that tab is, nothing else takes their place there, and they open again
when the tab is selected again. A second request waits for the first. Enter
confirms, Escape declines; a prompt opens with its suggested text selected.
A page that asks before it is left is answered with **Stay** or **Leave**; its
own wording is not shown.

HTTP sign-in (Basic, Digest) asks for a user name and password under the host
and the server's realm. What is typed is sent for that challenge and not
saved by the browser; the engine reuses it for the session. Over plain HTTP
the load is tried once more when the connection the server challenged on has
gone by the time the answer is sent.

A site whose certificate cannot be verified shows **This connection is not
private**. **Continue (unsafe)** accepts that certificate for that host until
the tab is closed; the address bar then shows a warning shield and the site
details say the site is not verified. Nothing is remembered across restarts.

When a page's process ends, the tab shows **This page stopped working** with
**Reload**, keeps its title, and does so in background tabs as well.

F11 gives the window the whole screen; a page can ask for the same (a video's
full-screen button). A website is then shown without the bars and the tabs.
Esc ends a page's request, F11 the window's. Selecting another tab ends a
page's request. The browser's own pages keep their bars.

Shortcuts include Ctrl+L, Ctrl+K, F6 and Alt+D address, Ctrl+T new tab, Ctrl+W
close, Ctrl+Tab and Ctrl+PgDn next tab, Ctrl+Shift+Tab and Ctrl+PgUp previous,
Ctrl+1 to Ctrl+8 a tab by its place and Ctrl+9 the last, Ctrl+Shift+T reopen,
Ctrl+N window, Ctrl+Shift+N private window, Ctrl+F find, Ctrl+D bookmark,
Ctrl+Shift+B the bookmarks bar, Ctrl+Shift+O bookmarks, Ctrl+H history, Ctrl+Shift+Delete the history page,
Ctrl+J downloads, Ctrl+S save, Ctrl+O open PDF, Ctrl+P print, Ctrl+R/F5 reload,
Ctrl+Shift+R/Ctrl+F5 reload without the cache, Alt+Left and Alt+Right back and
forward, F11 full screen, F12/Ctrl+Shift+I developer tools and Ctrl+Shift+J
console. The back and forward buttons of a mouse go back and forward.

A right-click in a page offers what fits the thing under the pointer and no
more. On a link: open in a new tab, a new window or a private window, save as
a file, copy the address. On an image or a media file: open in a new tab,
save, copy the address. In a text field: the corrections of a misspelled
word, cut, copy, paste, select all, undo, redo. On selected text: copy and a
search with the selected engine. On the page itself: bookmark, save, print,
translate, reader view, page source, inspect. Back, forward and reload are
the toolbar's, and find is in the browser's menu (Ctrl+F). A tab's menu has
pin, duplicate, reload, mute, copy address, move to a new window, groups,
containers, close, close the other tabs and close the tabs after it; pinned
tabs stay.

A page's dropdown (`<select>`) opens under its control, or over it where there
is more room, as wide as the control at least. Up and Down move through the
choices, a typed letter goes to the next choice that starts with it, Enter
chooses, Escape or a press beside the list closes it.

The zoom set on a site is kept for that site (its host and port) and applied
when a tab comes to it; 100% is kept for no site. Private windows remember it
for as long as they are open.

Settings → Browsing → **Save downloads to** chooses the folder downloads go
to, for the tabs that are open and those opened later; **Default** returns to
`Downloads`. With **Ask where to save each download** on, each
download asks for its place and name while it loads; it is written in the
downloads folder and moved to the chosen place when it is whole. Closing the
question gives the download up. PDFs that open in the browser are not asked
about.

A window a page opens stays that page's window: `window.opener` works, the two
can post messages to each other, and the window can close itself. A window the
page asked for as a popup (it named a size, or turned bars off, as sign-in and
payment windows do) opens as a small window of its own, above its opener and
at the size asked for, between 320 × 260 and 1600 × 1200. Its bar shows the
site's identity and its address, origin first, which cannot be edited, so that
the person can see which site asks for a password; it has no tabs, and what it
opens is a popup window too. Any other window a page opens, such as a link with
a target, is a tab in the opener's container, and closing itself brings the
opener back to the front. Either kind shares its opener's storage, a private
window's included, and starts loading only after it has the content filters,
page scripts and policy of any other tab. A page cannot close a tab a person
opened.

Spelling is checked in text fields and editable text with the dictionaries the
desktop's spelling service (enchant) finds, in the desktop's language unless
Settings → Browsing → **Check spelling** names others. A misspelled word is
underlined; its menu offers corrections, **Add to dictionary** (not in a
private window) and **Ignore spelling**. Without a dictionary on the computer nothing is marked, and
Settings says so.

A colour field (`<input type=color>`) opens a colour picker with a hex field;
the page sees each colour as it is tried and keeps the one chosen, or its old
one after Cancel. A click on a date, date-and-time, month or week field opens
a calendar beside it, and a time field a list of times; choosing sets the
field, Escape or a press elsewhere closes the picker, and the field can be
typed into all the while. Fields inside frames are typed.

**Move to new window** takes a tab with its page as it is: the page is not
loaded again and what plays goes on playing. A tab with a download running, a
pinned tab and a tab that was never loaded stay where they are.

The **bookmarks bar** (Settings → Browsing, or Ctrl+Shift+B) is a row under
the toolbar with the bookmarks kept in no folder and the outermost folders. A
bookmark opens in the tab on show, with the middle button in a tab behind; the
right button edits it. A folder opens a list of its bookmarks. What does not
fit is listed under the button at the end.

**Print** (Ctrl+P) lists the printers CUPS knows, with its default chosen,
and takes a number of copies and the pages (`1-3, 7`). A website is made into
a PDF by the engine and sent to the printer; that file is kept in the
session's runtime folder until `lp` has read it. A PDF is sent as it is.
**Save as PDF…** remains for a file instead of paper.

On the Bookmarks page the pencil beside a bookmark changes its name and its
folder (`Work / Reading` for a folder in a folder); one folder's bookmarks
are listed together. **Export…** writes all bookmarks as a bookmark file
(Netscape format) that Firefox, Chrome and the rest import, with the folders.

**Translate page** in a page's menu opens the page in Google Translate in a
new tab, into the desktop's language. The page's address is sent to that
service, which fetches the page itself, so it works for public pages only.
**Open reader view** opens the readable part of the page (its article, or
the block that holds most of its paragraphs) in a new tab: headings,
paragraphs, lists, quotes and code with their links, in one column, in the
theme's light or dark. A script in the browser's own script world takes the
text out and escapes it; none of the page's markup, styles, images or scripts
is carried over. A page with too little text says so instead.
**View page source** (Ctrl+U) opens the source the engine loaded as text in
a new tab; the text is kept in the session's runtime folder, readable by the
person alone, until they log out.

A tab whose page plays sound shows a moving wave over the top left corner of
its icon (three narrow bars in the colour of the tab's name, each edged in the
tab's own colour so that they stand clear of the icon), and a tab the person muted a crossed speaker there for as long as it
is muted. Under the pointer the mark becomes a small button with the symbol of
what a press does: a crossed speaker to mute, a speaker to let the tab sound
again, in the icon rail as in the sidebar and the top strip; its ground is a
circle. In the icon rail a tab's close button and its sound mark stand over
the tile's top right and top left corners, slightly past its edge, so a
click meant for the tab does not land on them. The wave stands still where
the desktop asks for less motion.

A right-click on a website belongs to the website: the page's menu opens where
the engine reports one, and nothing of the browser's where the site shows a
menu of its own. The browser's menu (new tab, containers, tab layout) opens
on the bars and on the browser's own pages. A middle click on a
link opens it in a new tab and stays on the page. A click on a `mailto:`,
`tel:` or `sms:` link is handed to the desktop's application for it; only the
person's own click is, and no other kind of link. A thin line along the top
of a loading page shows the engine's estimate of the load. A page that came
over HTTPS and took in images or scripts over plain HTTP shows a warning
shield and **Partly secure** in its site details.
Ctrl+scroll and Ctrl++/Ctrl+- zoom websites and PDFs; Ctrl+0 resets to 100%.
The PDF preview includes first/previous/next/last page, jump-to-page, fit width,
fit page, rotation and zoom from 25% to 400%. Page Up/Down and left/right arrows
change PDF pages; Home/End select the first/last page when a text field is not
focused. Renders run outside the UI thread and superseded requests are discarded.

Settings → Browsing offers middle-button autoscroll, off by default.
Middle-click a page background, then move above or below the marker. Click,
Escape, wheel input or leaving the window stops it; dragging with the middle
button stops on release. Middle-clicking links opens them in tabs.

Preferences, bookmarks and the last normal window's tab addresses are stored in
`$XDG_CONFIG_HOME/enzim-browser/session.json`. Dated history is in `history.sqlite` beside that session file; website storage
uses native profile directories under `$XDG_DATA_HOME/enzim-browser`.
Named browser profiles use generated IDs below `profiles/` in the corresponding
config/data/cache roots. `profiles.json` in the config root records display
names; the legacy default profile paths are preserved. `--skip-welcome` is an
explicit automation option for isolated test profiles.
Downloads go to `~/Downloads`. The native downloads panel shows
progress, pause/resume, cancellation, errors, retry and folder access in a compact
file list. Click a completed file to open it in the system's default application;
PDF downloads open automatically in the built-in reader. The file's action menu
also offers opening PDFs with the system app. Clicking an active file toggles
opening it when finished. Restored downloads never open automatically. Closing
the source tab does not stop its download. Completed files remain in the
normal window's list after restart; private windows do not save that list.

Pause/resume is offered while the browser remains open when an HTTP server
provides suitable byte ranges and a validator. Resumed responses are checked
before their retained prefix is joined to the new body. Servers that return a
complete replacement restart the file; invalid or incomplete ranges fail
visibly. Generated Blob files download natively and cannot be resumed.
The selected runtime still has a guard re-attestation failure in the HTTP resume
qualification after cross-site navigation; tracked in
engine issue #40.
Delayed 10/50/100 MiB Blob qualification passes.
