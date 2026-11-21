(() => {
    if (window.top !== window || location.protocol !== 'https:' || !isSecureContext)
        return null;
    if (globalThis.__enzimCredentials)
        return globalThis.__enzimCredentials.inspect();
    const token = Array.from(crypto.getRandomValues(new Uint32Array(4)), x => x.toString(16).padStart(8, '0')).join('');
    let enabled = true;
    let offerSave = true;
    let lastGesture = -Infinity;
    let lastCapture = -Infinity;
    let offered = false;
    // A suggestion the person dismissed (by typing, Escape or scrolling) stays
    // away from that field until they click it: pages and the view itself
    // refocus a field while it is used, which must not bring it back.
    let offeredTarget = '';
    let quiet = '';
    // A printable key was the last gesture: a submit then comes from the page
    // itself (validation as the person types), not from the person.
    let typing = false;
    let lastTrustedTarget = '';
    const ids = new WeakMap();
    const targets = new Map();
    let nextId = 1;
    const identify = node => {
        if (!ids.has(node)) ids.set(node, String(nextId++));
        const id = ids.get(node);
        targets.set(id, new WeakRef(node));
        if (targets.size > 256) targets.delete(targets.keys().next().value);
        return id;
    };
    const visible = element => {
        if (!(element instanceof HTMLInputElement) || element.disabled || element.readOnly || element.hidden || element.closest('[inert]')) return false;
        const box = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return box.width >= 8 && box.height >= 8 && box.right > 0 && box.bottom > 0 && box.left < innerWidth && box.top < innerHeight && style.visibility === 'visible' && style.display !== 'none' && Number(style.opacity) > 0;
    };
    const fields = (anchor = null) => {
        let active = anchor || document.activeElement;
        while (active?.shadowRoot?.activeElement) active = active.shadowRoot.activeElement;
        const form = active instanceof HTMLInputElement ? active.form : active?.closest?.('form');
        const root = form || active?.getRootNode?.() || document;
        const all = Array.from(root.querySelectorAll('input')).filter(visible);
        const passwords = all.filter(input => input.type === 'password' && input.autocomplete !== 'one-time-code');
        const password = passwords.find(input => input.autocomplete === 'current-password') || passwords.find(input => input.autocomplete !== 'new-password');
        // A verification code field says so in its autocomplete hint, or, as on many sites, only in its name or label.
        const codeHint = input => /(?:^|[^a-z])(?:otp|totp|2fa|mfa|one[-_ ]?time|two[-_ ]?factor|(?:verification|auth(?:enticator|entication)?|security)[-_ ]?code)/i.test([input.name, input.id, input.getAttribute('aria-label') || '', input.placeholder].join(' '));
        const otp = all.find(input => input.autocomplete.split(/\s+/).includes('one-time-code'))
            || all.find(input => ['text', 'tel', 'number'].includes(input.type) && !/username|email|password/.test(input.autocomplete) && codeHint(input) && (input.maxLength < 0 || (input.maxLength >= 6 && input.maxLength <= 8)));
        const username = all.find(input => input !== otp && input.autocomplete.split(/\s+/).includes('username')) || all.find(input => input !== otp && input.type === 'email') || all.find(input => input !== otp && input.type === 'text' && input.autocomplete !== 'one-time-code' && (!password || (input.compareDocumentPosition(password) & Node.DOCUMENT_POSITION_FOLLOWING)));
        const primary = password || otp || username;
        if (!form && primary?.form) return fields(primary);
        return {form, all, passwords, password, username, otp};
    };
    const actionOrigin = form => {
        try { return new URL(form?.action || location.href, location.href).origin; } catch { return ''; }
    };
    const post = value => {
        if (!enabled) return;
        try { window.webkit.messageHandlers.enzimCredentials.postMessage(value); } catch {}
    };
    const loginUsername = input => input && (input.type === 'email' || /(?:^|\s)(?:username|email)(?:\s|$)/.test(input.autocomplete) || /user|email|login/i.test(input.name + ' ' + input.id));
    const selected = form => form.password || form.otp || (loginUsername(form.username) ? form.username : null);
    const inspect = (suggest = false) => {
        const form = fields();
        const target = selected(form);
        let active = document.activeElement;
        while (active?.shadowRoot?.activeElement) active = active.shadowRoot.activeElement;
        const eligible = active && (active === form.password || active === form.otp || active === form.username && (form.password || loginUsername(active)));
        const rect = suggest && document.hasFocus() && document.visibilityState === 'visible' && eligible && visible(active) && (active === form.otp || !form.password || !form.password.value) ? active.getBoundingClientRect() : null;
        if (rect && target) lastTrustedTarget = identify(target);
        return {kind: 'ready', document: token, origin: location.origin, action: actionOrigin(form.form), target: target ? identify(target) : '', password: Boolean(form.password), otp: Boolean(form.otp), username_only: Boolean(!form.password && !form.otp && loginUsername(form.username)), focus: rect ? {x: rect.x, y: rect.y, width: rect.width, height: rect.height, viewport_width: innerWidth, viewport_height: innerHeight} : null};
    };
    const notify = event => {
        const value = inspect(Boolean(event?.isTrusted && performance.now() - lastGesture < 1200));
        if (value.focus && quiet && value.target === quiet && event?.type !== 'click') value.focus = null;
        offered = value.focus !== null;
        if (offered) { offeredTarget = value.target; quiet = ''; }
        post(value);
    };
    const dismiss = () => {
        if (!offered) return;
        offered = false;
        quiet = offeredTarget;
        post({kind: 'dismiss', document: token, origin: location.origin});
    };
    const onPointer = event => { if (event.isTrusted) { lastGesture = performance.now(); typing = false; } };
    const capture = event => {
        if (!enabled || !offerSave || !event.isTrusted || performance.now() - lastGesture > 1200 || performance.now() - lastCapture < 300) return;
        if (event.type === 'submit' && typing) return;
        const form = fields();
        const fresh = form.passwords.filter(input => input.autocomplete === 'new-password');
        const password = fresh.length ? fresh[0] : form.password;
        if (!password || !password.value || password.value.length > 4096 || actionOrigin(form.form) !== location.origin) return;
        if (fresh.length > 1 && fresh.some(input => input.value !== password.value)) return;
        const username = form.username?.value || '';
        if (username.length > 4096) return;
        lastCapture = performance.now();
        post({kind: 'submitted', document: token, origin: location.origin, action: actionOrigin(form.form), username, password: password.value});
    };
    const onKey = event => {
        if (!enabled || !event.isTrusted) return;
        lastGesture = performance.now();
        typing = event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey;
        if (event.key === 'Escape') dismiss();
        if (event.key === 'Enter' && event.target instanceof HTMLInputElement) capture(event);
    };
    const onClick = event => {
        if (!enabled || !event.isTrusted) return;
        lastGesture = performance.now();
        typing = false;
        const input = event.composedPath().find(node => node instanceof HTMLInputElement);
        if (input && visible(input)) { notify(event); return; }
        dismiss();
        const control = event.composedPath().find(node => node instanceof Element && node.matches('button,input[type=submit],[role=button]'));
        if (!control) return;
        const form = control.closest('form');
        if (form) {
            const password = Array.from(form.querySelectorAll('input')).find(input => input.type === 'password' && visible(input));
            if (password && (control.type === 'submit' || control.getAttribute('role') === 'button')) capture(event);
        }
    };
    let attached = false;
    const attach = () => {
        if (attached) return;
        attached = true;
        document.addEventListener('focusin', notify, true);
        document.addEventListener('pointerdown', onPointer, true);
        document.addEventListener('input', dismiss, true);
        document.addEventListener('scroll', dismiss, true);
        window.addEventListener('resize', dismiss);
        document.addEventListener('submit', capture, true);
        document.addEventListener('keydown', onKey, true);
        document.addEventListener('click', onClick, true);
    };
    attach();
    globalThis.__enzimCredentials = Object.freeze({
        inspect,
        configure(save) { offerSave = Boolean(save); enabled = true; attach(); notify(); },
        disable() {
            enabled = false;
            attached = false;
            document.removeEventListener('focusin', notify, true);
            document.removeEventListener('pointerdown', onPointer, true);
            document.removeEventListener('input', dismiss, true);
            document.removeEventListener('scroll', dismiss, true);
            window.removeEventListener('resize', dismiss);
            offered = false;
            document.removeEventListener('submit', capture, true);
            document.removeEventListener('keydown', onKey, true);
            document.removeEventListener('click', onClick, true);
        },
        fill(expected, username, password, otp) {
            if (!enabled || expected.document !== token || expected.origin !== location.origin || window.top !== window || !isSecureContext) return false;
            if (lastTrustedTarget !== expected.target && targets.get(lastTrustedTarget)?.deref()?.isConnected) return false;
            const anchor = targets.get(expected.target)?.deref();
            if (!anchor || !anchor.isConnected || !visible(anchor)) return false;
            const current = fields(anchor);
            if (actionOrigin(current.form) !== expected.origin || expected.target !== (selected(current) ? identify(selected(current)) : '')) return false;
            const set = (input, value) => {
                input.focus({preventScroll: true});
                if (!enabled || location.origin !== expected.origin || !input.isConnected || !visible(input) || actionOrigin(current.form) !== expected.origin) return false;
                const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
                setter.call(input, value);
                input.dispatchEvent(new Event('input', {bubbles: true, composed: true}));
                input.dispatchEvent(new Event('change', {bubbles: true, composed: true}));
                return input.isConnected && input.value === value;
            };
            if (otp !== null) {
                if (!current.otp) return false;
                return set(current.otp, otp);
            }
            if (expected.username_only) {
                if (current.password || current.otp || !loginUsername(current.username) || !visible(current.username)) return false;
                return set(current.username, username);
            }
            if (!current.password || current.password.value) return false;
            if (current.username && !set(current.username, username)) return false;
            if (location.origin !== expected.origin || expected.document !== token || !current.password.isConnected || !visible(current.password) || current.password.value || current.username && current.username.value !== username || actionOrigin(current.form) !== expected.origin) return false;
            return set(current.password, password);
        },
    });
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', notify, {once: true});
    else notify();
    return inspect();
})()
