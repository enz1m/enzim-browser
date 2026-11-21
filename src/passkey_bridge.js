// Relays passkey requests from the page's world to the browser, and the
// browser's replies back. It runs in an isolated world: the handler below
// reports the frame and origin the engine recorded, whatever the page says.
(() => {
    if (globalThis.__enzimPasskeys || location.protocol !== 'https:' || !isSecureContext)
        return;
    const handler = globalThis.webkit?.messageHandlers?.enzimPasskeys;
    if (!handler) return;
    addEventListener('enzim-passkey-request', event => {
        if (typeof event.detail !== 'string' || event.detail.length > 65536) return;
        let message;
        try { message = JSON.parse(event.detail); } catch { return; }
        try { handler.postMessage(message); } catch {}
    }, true);
    globalThis.__enzimPasskeys = Object.freeze({
        reply(message) {
            dispatchEvent(new CustomEvent('enzim-passkey-response', {detail: JSON.stringify(message)}));
            return true;
        },
    });
})();
