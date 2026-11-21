(() => {
    const key = Symbol.for('enzim.devtools.console');
    if (globalThis[key]) return;
    let remaining = 500;
    let active = true;
    const originals = new Map();
    const installed = new Map();
    const preview = (value, depth = 0) => {
        if (typeof value === 'string') return value.slice(0, 2000);
        if (typeof value === 'function') return '[Function]';
        if (value === null || typeof value !== 'object') return String(value).slice(0, 500);
        if (depth > 1) return Array.isArray(value) ? '[…]' : '{…}';
        try {
            const fields = Reflect.ownKeys(value).slice(0, 20);
            return '{' + fields.map(key => {
                const field = Object.getOwnPropertyDescriptor(value, key);
                return String(key) + ': ' + (field && 'value' in field ? preview(field.value, depth + 1) : '[Getter]');
            }).join(', ') + '}';
        } catch (_) { return '[Object]'; }
    };
    const send = (level, args) => {
        if (!active || remaining-- <= 0) return;
        try {
            window.webkit.messageHandlers.enzimConsole.postMessage({level, text: args.slice(0, 10).map(value => preview(value)).join(' ').slice(0, 4000)});
        } catch (_) {}
    };
    for (const level of ['log', 'info', 'warn', 'error', 'debug']) {
        const original = console[level];
        const wrapped = function (...args) { send(level, args); return Reflect.apply(original, this, args); };
        originals.set(level, original);
        installed.set(level, wrapped);
        console[level] = wrapped;
    }
    const originalAssert = console.assert;
    const wrappedAssert = function (condition, ...args) {
        if (!condition) send('error', ['Assertion failed:', ...args]);
        return Reflect.apply(originalAssert, this, [condition, ...args]);
    };
    originals.set('assert', originalAssert);
    installed.set('assert', wrappedAssert);
    console.assert = wrappedAssert;
    const error = event => send('error', [event.message, event.filename, event.lineno]);
    const rejection = event => send('error', ['Unhandled promise rejection:', event.reason]);
    addEventListener('error', error);
    addEventListener('unhandledrejection', rejection);
    Object.defineProperty(globalThis, key, {configurable: true, value: {stop() {
        active = false;
        for (const [level, wrapped] of installed) {
            if (console[level] === wrapped) console[level] = originals.get(level);
        }
        removeEventListener('error', error);
        removeEventListener('unhandledrejection', rejection);
        delete globalThis[key];
    }}});
})()
