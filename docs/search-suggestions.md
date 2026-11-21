# Search suggestions

Search suggestions are on by default. Turn them off in Settings → Browsing → Search suggestions.
History and bookmark matches stay on the device and appear before provider
suggestions. An empty first-run profile has no seeded websites.

The address bar, new-tab dialog and start-page search share the same results
and keyboard navigation. A search suggestion always searches with the selected
engine, including when its wording resembles an address. Results arriving after
a keyboard row was selected cannot change what Enter will open.

Supported built-in providers are DuckDuckGo, Google, Bing, Startpage, Brave Search
and Wikipedia. Custom engines have no suggestion endpoint. If a provider is
unavailable or rate-limits requests, local matches and the typed search remain
usable; the browser does not retry automatically or use another provider.

After at least two search characters and a 250 ms typing pause, the browser sends
the query directly to the selected provider over HTTPS. URL/address-like input,
empty input and private windows do not request provider suggestions. Requests
have no browser cookies or referrer, follow the active container's configured
proxy, do not follow redirects, and are cancelled when the input changes or
closes. Responses are limited to 32 KiB and six bounded text results. Typing-only
queries and results are not saved to history or session state.

Endpoint references: [Firefox search configuration](https://github.com/mozilla-firefox/firefox/blob/main/services/settings/dumps/main/search-config-v2.json),
[Chromium search-engine data](https://chromium.googlesource.com/external/search_engines_data/+/refs/heads/main/definitions/prepopulated_engines.json)
and the providers' own HTTPS suggestion services. Firefox also documents the
[query-sharing behavior](https://support.mozilla.org/en-US/kb/search-suggestions-firefox).
Live qualification distinguishes provider responses from temporary rate limits.
