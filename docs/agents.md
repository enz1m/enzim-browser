# Agents

An AI agent can work the browser through the enzUI agent center
(`enzui-mcp-center`): list, open, select and close tabs and windows, navigate,
read a page, click, type, fill forms, answer dialogs, run a script and capture
the page. The functions follow the names of Chrome's DevTools MCP server, so an
agent that knows that server knows these.

The browser is not an MCP server itself. The center is: an agent connects to
`enzui-mcp`, finds the browser with `enzui_apps`, lists its functions with
`enzui_functions`, reads one with `enzui_function` and calls it with
`enzui_call`. The center asks you before each function runs for an agent, once,
for the session or always (a function that closes something asks again for
other tabs or windows), and keeps a history of the calls. The browser takes
part only when the center has given it a credential; without one nothing is
listening.

## Turning it on

With the center running (`enzui-center run`, or its user service on enzimOS):

```sh
enzui-center enroll app --id org.enzim.Browser --name "enzim Browser" \
  --output ~/.local/state/enzui/apps/org.enzim.Browser.credential.json
```

Restart the browser. It finds the credential there, or wherever
`ENZUI_APP_CREDENTIAL` points, and connects to the center's socket
(`$XDG_RUNTIME_DIR/enzui/center.sock`, or `ENZUI_CENTER_SOCKET`). An agent is
enrolled separately (`enzui-center enroll agent ...`) and runs `enzui-mcp` with
its own credential; see the center's README.

## Functions

| Function | What it does |
|---|---|
| `list_windows` | Windows, with their tab counts and selected page |
| `list_containers` | Containers: id, name, whether a proxy carries their traffic, whether agents may use them |
| `list_pages` | Every tab: page id, window, address, title, container, selected, loaded, loading, an open dialog; `container` lists one container's tabs |
| `select_page` | Shows a tab in its window and waits for it to load; `bring_to_front` raises the window |
| `new_page` | Opens an address in a new tab and waits for the load; `container` opens it in a container, `background` leaves it unloaded until used |
| `close_page`, `close_window` | Close a tab or a window |
| `navigate_page` | An address, back, forward or reload, and waits for the load |
| `new_window` | Opens a window; `container` puts its tab in a container |
| `take_snapshot` | A text outline of the page with a uid for each element; long pages come in parts |
| `take_screenshot` | A JPEG of the visible page, or the whole page |
| `click`, `hover` | Pointer input at the element's centre |
| `fill`, `fill_form` | Text fields, choice lists and checkboxes |
| `type_text`, `press_key` | Keyboard input, such as `Enter` or `Control+A` |
| `scroll` | An element into view, or the page by a distance |
| `wait_for` | Until some text appears |
| `evaluate_script` | A JavaScript function in the page, with elements as arguments |
| `handle_dialog` | Accepts or dismisses an alert, confirmation or prompt |

A page id names a tab for as long as it is open, through navigation and moves
between windows. A uid names an element of the document the snapshot was taken
from; after the page navigates, take a new snapshot.

## What to expect

- A tab that is not loaded (one restored from the last session, or opened in the
  background) loads when an agent uses it.
- Reading and scripting work on any loaded tab. Pointer and key input, like
  yours, reach only the tab shown in its window, so `click`, `hover`,
  `type_text` and `press_key` select the tab first.
- While a browser panel or menu is open over the page, the page takes no keys;
  `type_text` and `press_key` say so instead of typing elsewhere.
- Private windows are not listed and cannot be reached.
- A container is named by its `container_id` (a string) or its name, in any
  case; `"default"`, or leaving `container` out, is no container. A tab opened
  in a container has that container's cookies, sign-ins, storage and proxy.
  `list_containers` says whether a container uses a proxy, never its address
  or sign-in.
- Turning off Allow agents in a container's settings closes it to agents: its
  tabs are not listed or counted, a window whose shown tab is in it reports no
  selected page, functions aimed at its tabs are refused, pages cannot be
  opened in it, and a window holding its tabs cannot be closed by an agent.
- One browser process connects at a time: a second profile running at the same
  time is refused by the center.
- The center returns a screenshot as an image to agents whose MCP client shows
  images, and as base64 text otherwise.

The browser's own [assistant](assistant.md) offers its model these same
functions, with the same limits.

`tools/check_agent_center.py` drives every function against a private center;
see [development](development.md).
