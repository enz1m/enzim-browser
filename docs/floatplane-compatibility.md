# Floatplane verification

On 2026-10-31, the login page at https://floatplane.com/login and its www host
loaded in enzim, but the Cloudflare Turnstile widget remained at Verifying.
The selected engine runtime was `343bc9b972-pgo-v3`.

A separate fresh enzim profile reproduced the failure with ads/trackers blocked
nowhere, ITP disabled, third-party cookies allowed, default native user agent,
JavaScript/WebGL enabled, no proxy, and the credential manager disabled.
Developer tools were closed during the initial observation. Afterwards, the
environment reported a secure context, available WebAssembly/WebGL, cookies
available and navigator.webdriver=false. Native TLS showed no certificate
errors. Those checks do not establish complete challenge compatibility.

A headed Chrome 156.0.8078.79 control with a new profile, on the same desktop and
network, reached Success without submitting login credentials. Test profiles
and public-login-page screenshots remain outside Git; normal profiles and
privacy settings were not altered for this investigation.

WPE issue #46
contains reproduction details. The native UA's Version/60.5 token is recorded
as an observation, not an established cause. No UA spoof, CAPTCHA solver or
silent privacy exception has been shipped.

Cloudflare documents custom/embedded browsers as having limited support and
lists script loading, storage and consistent user-agent behavior as relevant:
[supported browsers](https://developers.cloudflare.com/cloudflare-challenges/reference/supported-browsers/),
[WebView configuration](https://developers.cloudflare.com/turnstile/get-started/mobile-implementation/).
Verification success in enzim is still pending an engine/embedding investigation.
