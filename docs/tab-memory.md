# Tab memory management

The browser enables WPE GPU-process compositing on the native owner thread
before creating a session. GPU frame presentation remains enabled. The GPU
process paints and composites every page and creates the buffers they are
shown in, so a web process loads the graphics driver only when its page uses
WebGL or plays video. The selected
runtime is pinned in [DEPENDENCIES.json](../DEPENDENCIES.json). Its source
patches and source bundle are maintained in the WebKit SDK.

A shared native-readiness deadline runs the engine’s tab-memory policy once a
second. It reads system/cgroup availability and kernel memory pressure. A tab hidden
for four seconds, and neither loading nor playing audio, gets one garbage
collection for that hidden period. It returns what the last load left behind
(5 to 10 MiB on the measured pages) and keeps compiled code, images and caches,
so switching back is unaffected. Eight seconds after its last page is hidden, a
web process trims itself in the engine: bytecode and compiled code, decoded
images and script text and non-critical caches go, while style, layout and
tiles stay. The page is back on screen as fast as before, and its scripts are
parsed again when they next run (about 20 ms for 1500 functions). The trim also
gives back the pages inside the JavaScript heap's blocks that hold no live
object and stops keeping the pages of resources mapped from the disk cache
resident. From then on, while the process shows no page, a script whose text
has not been read for thirty seconds is parked: one mapped from the disk cache
lets go of its pages, any other of 16 KiB or more keeps a compressed copy of
its text, which is expanded when a function is next compiled or printed (about
3 ms per megabyte). A page that is
loading or playing postpones the trim. Hidden
tabs release caches on a schedule set in Settings → Memory → Background tabs;
a tab shown again after its first release is repainted first (on an image grid,
blank for about 30–100 ms instead of complete at once). The release adds little
to the trim: about 12 MB on a photo grid and 46 MB on YouTube.

| Setting | Most recently shown hidden tabs | Other hidden tabs | Later releases | Discard under moderate pressure |
|---|---|---|---|---|
| Aggressive (default) | — | 15 s | 5 min, then every 30 min | after 10 min |
| Balanced | 5 most recent: 1 min | 15 s | 5 min, then every 30 min | after 10 min |
| Relaxed | 10 most recent: 5 min | 1 min | 10 min, then every 30 min | after 30 min |

A tab that is no longer among the most recent ones releases on the other tabs'
schedule, at once if that time has passed.
These releases preserve the running document. Hidden GPU frames are dropped
without waiting for this schedule. A release drops the tab's tiles, and the GPU
process then asks the graphics driver for that memory, which the driver would
otherwise keep until something else is drawn. The window's own renderer gives
back the textures of an animation ten seconds after it ends.

Under critical pressure, the least recently visible eligible tab is discarded,
at most one per tick. Under moderate pressure it must have been hidden for
ten minutes. Visible tabs, audio-playing tabs, pinned tabs and tabs with user
edits are protected. Pending native operations, active downloads, loading pages
and open developer tools also prevent discarding. Pin protection is published
through the shared admission state before command processing.

Form-edit detection uses one trusted input notification per document, including
embedded frames, and carries only a boolean. It has no timer and sends no field
contents. The flag remains conservative until explicit navigation. Native
credential filling and IME editing also protect the page.

Discarding hides the page before consuming its session, cancels favicon work,
and drops held compositor frames. The tab retains the engine’s history/form/
scroll snapshot in memory. It keeps its title and address in the browser’s
existing tab state. Activating it creates a new session in the same profile,
reapplies privacy, permissions, theme, zoom and content filters, then restores
the snapshot. Resizing a discarded background tab does not revive it. Theme,
zoom and vault-refresh changes preserve discarded state. Retained privacy and
permission preferences are reapplied when restoring the session.

The default policy has no age-only Memory Saver timeout. Ordinary background
tabs remain loaded unless memory pressure calls for discarding. Closing a tab
also drops its retained snapshot. Snapshots are not added to session JSON or
written to disk by this feature.

Qualification covers native process exit, same-profile reconstruction, retained
tab metadata, actual policy dispatch, protected tabs, trusted versus synthetic
edit notifications, idle discarded workers, background resizing and activation.
The first tick runs without requiring a previous readiness query, then stays
on a one-second monotonic deadline. Its moving-initial-deadline fault is covered
by a deliberate negative control. The engine’s two discard checks cover history,
scroll/form restoration and hidden timer throttling. Deliberate retained-session,
synthetic-edit and missing-restore faults are detected, and restored checks pass.
Existing credential
qualification also passes on the new runtime.
