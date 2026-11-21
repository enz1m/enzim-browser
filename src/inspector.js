(() => {
    const root = globalThis;
    const token = Symbol.for('enzim.devtools.dom');
    const state = root[token] ||= {ids: new WeakMap(), nodes: new Map(), next: 1};
    const identify = element => {
        if (!state.ids.has(element)) {
            if (state.nodes.size >= 4096) {
                for (const [id, reference] of state.nodes) if (!reference.deref()?.isConnected) state.nodes.delete(id);
                if (state.nodes.size >= 4096) state.nodes.delete(state.nodes.keys().next().value);
            }
            const id = state.next++;
            state.ids.set(element, id);
            state.nodes.set(id, new WeakRef(element));
        }
        const id = state.ids.get(element);
        if (!state.nodes.has(id)) state.nodes.set(id, new WeakRef(element));
        return id;
    };
    const summary = element => ({node: identify(element), tag: element.localName, id: element.id.slice(0, 120), classes: Array.from(element.classList).slice(0, 4).join('.').slice(0, 160), children: element.childElementCount});
    root.__enzimSnapshot = element => {
        if (!element?.isConnected) throw new Error('This element is no longer in the document. Inspect again.');
        const style = getComputedStyle(element), rect = element.getBoundingClientRect();
        const parents = [];
        for (let parent = element.parentElement; parent && parents.length < 16; parent = parent.parentElement) parents.unshift(summary(parent));
        return {
            ...summary(element), parents,
            attributes: Array.from(element.attributes).slice(0, 24).map(a => [a.name, a.value.slice(0, 256)]),
            childNodes: Array.from(element.children).filter(child => child !== state.overlay).slice(0, 80).map(summary),
            text: Array.from(element.childNodes).filter(child => child.nodeType === Node.TEXT_NODE).slice(0, 20).map(child => child.textContent.slice(0, 300)).join(' ').trim().slice(0, 1000),
            bounds: {x: rect.x, y: rect.y, width: rect.width, height: rect.height},
            styles: Object.fromEntries(['display', 'position', 'width', 'height', 'margin', 'padding', 'color', 'background-color', 'font', 'border', 'border-radius', 'overflow', 'z-index', 'opacity', 'transform'].map(key => [key, style.getPropertyValue(key)])),
            inline: Array.from(element.style).slice(0, 32).map(key => [key, element.style.getPropertyValue(key), element.style.getPropertyPriority(key)])
        };
    };
    const select = element => {root.__enzimSelected = element; return root.__enzimSnapshot(element);};
    root.__enzimInspect = selector => {
        const element = selector.trim() ? document.querySelector(selector) : (root.__enzimSelected?.isConnected ? root.__enzimSelected : document.documentElement);
        if (!element) throw new Error('No element matches this selector.');
        return select(element);
    };
    root.__enzimInspectNode = id => {
        const element = state.nodes.get(id)?.deref();
        if (!element?.isConnected) throw new Error('This element changed or was removed. Refresh the inspection.');
        return select(element);
    };
    root.__enzimCancelPick = () => {
        if (state.pick) document.removeEventListener('pointerdown', state.pick, true);
        if (state.move) document.removeEventListener('pointermove', state.move, true);
        if (state.frame) cancelAnimationFrame(state.frame);
        state.pick = state.move = state.frame = state.target = undefined;
        state.overlay?.remove(); state.overlay = undefined;
    };
    root.__enzimBeginPick = () => {
        root.__enzimCancelPick();
        state.move = event => {
            state.target = event.target;
            if (state.frame) return;
            state.frame = requestAnimationFrame(() => {
                state.frame = undefined;
                if (!(state.target instanceof Element)) return;
                const box = state.target.getBoundingClientRect();
                if (!state.overlay) {
                    state.overlay = document.createElement('div');
                    state.overlay.setAttribute('aria-hidden', 'true');
                    document.documentElement.append(state.overlay);
                }
                state.overlay.style.cssText = `position:fixed!important;pointer-events:none!important;z-index:2147483647!important;box-sizing:border-box!important;background:rgba(66,153,225,.18)!important;border:1px solid #4299e1!important;left:${box.x}px!important;top:${box.y}px!important;width:${box.width}px!important;height:${box.height}px!important;`;
            });
        };
        state.pick = event => {
            if (!event.isTrusted || !(event.target instanceof Element)) return;
            event.preventDefault(); event.stopImmediatePropagation();
            const target = event.target;
            root.__enzimCancelPick();
            window.webkit.messageHandlers.enzimInspect.postMessage(select(target));
        };
        document.addEventListener('pointerdown', state.pick, true);
        document.addEventListener('pointermove', state.move, true);
        return true;
    };
    root.__enzimDisposeInspector = () => {
        root.__enzimCancelPick();
        root.__enzimSelected = undefined;
        state.nodes.clear(); state.ids = new WeakMap();
    };
    return true;
})()
