(() => {
    // The engine draws no calendar or clock for a date or time field. A
    // person's click on one is reported with what the field holds, and the
    // browser shows a picker beside it; what is chosen there is set here.
    // Fields in frames are left to typing: their place in the window is not
    // known here.
    if (window.top !== window) return;
    const kinds = ['date', 'time', 'datetime-local', 'month', 'week'];
    let field = null;
    const post = message => window.webkit.messageHandlers.enzimDateField.postMessage(message);
    document.addEventListener('click', event => {
        if (!event.isTrusted || event.button !== 0) return;
        const input = event.composedPath().find(node => node instanceof HTMLInputElement && kinds.includes(node.type));
        if (!input || input.disabled || input.readOnly) return;
        const box = input.getBoundingClientRect();
        field = input;
        post({
            kind: 'open',
            type: input.type,
            value: input.value.slice(0, 32),
            min: input.min.slice(0, 32),
            max: input.max.slice(0, 32),
            rect: [box.left, box.top, box.width, box.height],
        });
    }, true);
    // A picker left hanging over a page that moved under it points nowhere.
    addEventListener('scroll', () => { if (field) post({ kind: 'close' }); }, { capture: true, passive: true });
    // Called by the browser, in this world, with what the person chose. The
    // field last clicked takes it, as if typed: the page hears both events.
    globalThis.enzimSetDateField = value => {
        if (!field || !field.isConnected) return false;
        field.value = value;
        field.dispatchEvent(new Event('input', { bubbles: true }));
        field.dispatchEvent(new Event('change', { bubbles: true }));
        return true;
    };
})();
