(() => {
    let reported = false;
    addEventListener('pageshow', () => {
        if (reported) window.webkit.messageHandlers.enzimTabEdits.postMessage(true);
    });
    document.addEventListener('input', event => {
        if (reported || !event.isTrusted || !event.composedPath().some(node => node instanceof Element && node.matches('input,textarea,[contenteditable]'))) return;
        reported = true;
        // This bridge can only protect a tab; it never sends field contents or clears protection.
        window.webkit.messageHandlers.enzimTabEdits.postMessage(true);
    }, true);
})();
