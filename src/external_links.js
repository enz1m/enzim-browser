(() => {
    // A link to another application (mail, phone) is not a page to load. The
    // person's own click on one is reported, and the browser hands the link
    // to the desktop; a click made by a script is not.
    document.addEventListener('click', event => {
        if (!event.isTrusted || event.button !== 0) return;
        const link = event.composedPath().find(node => node instanceof HTMLAnchorElement && node.href);
        if (!link || link.href.length > 2048 || !/^(mailto|tel|sms):/i.test(link.href)) return;
        event.preventDefault();
        window.webkit.messageHandlers.enzimExternalLink.postMessage(link.href);
    }, true);
})();
