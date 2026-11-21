// The agent's helper in its own isolated world: the page cannot see or replace
// it. It names elements with uids that hold for this document only; a uid from
// an earlier document fails as stale instead of naming another element.
(() => {
  if (globalThis.__enzimAgent) return globalThis.__enzimAgent;
  const documentToken = Math.random().toString(36).slice(2, 8);
  const byUid = new Map();
  const uidOf = new WeakMap();
  let next = 1;
  const uid = (element) => {
    let id = uidOf.get(element);
    if (!id) {
      id = `${documentToken}_${next++}`;
      uidOf.set(element, id);
      byUid.set(id, new WeakRef(element));
    }
    return id;
  };
  const element = (id) => {
    if (typeof id !== 'string' || !id.startsWith(documentToken + '_')) {
      throw new Error('stale_uid: this uid belongs to another document; take a new snapshot');
    }
    const found = byUid.get(id)?.deref();
    if (!found || !found.isConnected) {
      throw new Error('stale_uid: the element is no longer in the page; take a new snapshot');
    }
    return found;
  };
  const implicitRole = (el) => {
    const tag = el.localName;
    const type = (el.getAttribute('type') || '').toLowerCase();
    switch (tag) {
      case 'a': return el.hasAttribute('href') ? 'link' : '';
      case 'button': return 'button';
      case 'select': return el.multiple || el.size > 1 ? 'listbox' : 'combobox';
      case 'textarea': return 'textbox';
      case 'img': return el.getAttribute('alt') === '' ? '' : 'img';
      // A drawing has no text of its own; it is shown as a picture.
      case 'canvas': return el.width * el.height >= 4096 ? 'img' : '';
      case 'h1': case 'h2': case 'h3': case 'h4': case 'h5': case 'h6': return 'heading';
      case 'nav': return 'navigation';
      case 'main': return 'main';
      case 'form': return 'form';
      case 'dialog': return 'dialog';
      case 'summary': return 'button';
      case 'option': return 'option';
      case 'li': return 'listitem';
      case 'ul': case 'ol': return 'list';
      case 'table': return 'table';
      case 'iframe': return 'iframe';
      case 'label': return 'label';
      case 'input':
        if (type === 'hidden') return '';
        if (['button', 'submit', 'reset', 'image'].includes(type)) return 'button';
        if (type === 'checkbox') return 'checkbox';
        if (type === 'radio') return 'radio';
        if (type === 'range') return 'slider';
        if (type === 'search') return 'searchbox';
        return 'textbox';
    }
    if (el.isContentEditable && !el.parentElement?.isContentEditable) return 'textbox';
    return '';
  };
  const text = (value, limit = 120) => {
    const clean = String(value ?? '').replace(/\s+/g, ' ').trim();
    return clean.length > limit ? clean.slice(0, limit - 1) + '…' : clean;
  };
  const labelled = (el) => {
    const by = el.getAttribute('aria-labelledby');
    if (by) {
      const joined = by.split(/\s+/).map((id) => el.ownerDocument.getElementById(id)?.textContent || '').join(' ');
      if (joined.trim()) return joined;
    }
    return el.getAttribute('aria-label') || '';
  };
  const name = (el, role) => {
    const aria = labelled(el);
    if (aria) return text(aria);
    if (el.labels && el.labels.length) return text(Array.from(el.labels).map((l) => l.textContent).join(' '));
    if (el.localName === 'img' || (el.localName === 'input' && el.type === 'image')) return text(el.getAttribute('alt') || el.getAttribute('title'));
    if (el.localName === 'canvas') return 'a drawing; its content shows only in a screenshot';
    if (['button', 'link', 'heading', 'option', 'listitem', 'label', 'checkbox', 'radio'].includes(role) && el.localName !== 'input') {
      return text(el.innerText || el.textContent);
    }
    if (el.localName === 'input' && ['button', 'submit', 'reset'].includes(el.type)) return text(el.value);
    return text(el.getAttribute('placeholder') || el.getAttribute('title') || '');
  };
  const hidden = (el) => {
    if (el.getAttribute('aria-hidden') === 'true') return true;
    const style = getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse') return true;
    return false;
  };
  const focusable = (el) => el.tabIndex >= 0 && !el.disabled;
  const states = (el, role) => {
    const out = [];
    if (role === 'heading') out.push(`level=${el.localName[1] || el.getAttribute('aria-level') || ''}`);
    if ('checked' in el && (role === 'checkbox' || role === 'radio')) out.push(el.checked ? 'checked' : 'unchecked');
    if (el.getAttribute('aria-checked')) out.push(`checked=${el.getAttribute('aria-checked')}`);
    if (el.getAttribute('aria-expanded')) out.push(`expanded=${el.getAttribute('aria-expanded')}`);
    if (el.getAttribute('aria-selected') === 'true' || (el.localName === 'option' && el.selected)) out.push('selected');
    if (el.disabled || el.getAttribute('aria-disabled') === 'true') out.push('disabled');
    if (el.required) out.push('required');
    if (el === el.ownerDocument.activeElement) out.push('focused');
    if (['textbox', 'searchbox', 'combobox', 'slider'].includes(role)) {
      if (el.isContentEditable) out.push(`value="${text(el.innerText, 200)}"`);
      else if (el.localName === 'select') out.push(`value="${text(el.selectedOptions?.[0]?.textContent || '', 200)}"`);
      else if (el.type === 'password') out.push(el.value ? 'value=(hidden)' : 'value=""');
      else out.push(`value="${text(el.value, 200)}"`);
    }
    if (role === 'link') {
      const href = el.getAttribute('href') || '';
      if (href && !href.startsWith('javascript:')) out.push(`url="${text(el.href, 160)}"`);
    }
    return out;
  };
  const roleOf = (el) => (el.getAttribute('role') || '').split(/\s+/)[0] || implicitRole(el);
  const interesting = (el, role) => {
    if (role && !['generic', 'presentation', 'none', 'list', 'listitem', 'form', 'table'].includes(role)) return true;
    if (focusable(el) && el !== el.ownerDocument.body) return true;
    if (el.onclick || el.getAttribute('onclick')) return true;
    return false;
  };
  const snapshot = (options) => {
    const verbose = !!options.verbose;
    const lines = [];
    let budget = options.maxBytes || 48000;
    let truncated = false;
    let skipped = 0;
    const start = options.offset || 0;
    let index = 0;
    const emit = (depth, line) => {
      index++;
      if (index <= start) { skipped++; return true; }
      const full = '  '.repeat(Math.min(depth, 30)) + line;
      budget -= full.length + 1;
      if (budget < 0) { truncated = true; return false; }
      lines.push(full);
      return true;
    };
    const walkText = (node, depth) => {
      const value = text(node.textContent, 200);
      // Text carries its element's uid, so that any visible text can be pointed at.
      if (value) return emit(depth, `uid=${uid(node.parentElement)} StaticText "${value}"`);
      return true;
    };
    const walk = (node, depth) => {
      if (truncated) return;
      for (let child = node.firstChild; child && !truncated; child = child.nextSibling) {
        if (child.nodeType === Node.TEXT_NODE) {
          if (!walkText(child, depth)) return;
          continue;
        }
        if (child.nodeType !== Node.ELEMENT_NODE) continue;
        const el = child;
        if (['script', 'style', 'noscript', 'template', 'head', 'meta', 'link'].includes(el.localName)) continue;
        if (hidden(el)) continue;
        const role = roleOf(el);
        let nextDepth = depth;
        if (verbose || interesting(el, role)) {
          const label = name(el, role);
          const parts = [`uid=${uid(el)}`, role || el.localName];
          if (label) parts.push(`"${label}"`);
          parts.push(...states(el, role));
          if (!emit(depth, parts.join(' '))) return;
          nextDepth = depth + 1;
          // A control's own text is in its name; its subtree adds nothing but noise.
          if (['button', 'link', 'textbox', 'searchbox', 'combobox', 'option', 'heading', 'img', 'checkbox', 'radio', 'slider'].includes(role) && !verbose) continue;
        }
        if (el.shadowRoot) walk(el.shadowRoot, nextDepth);
        if (el.localName === 'iframe') {
          let inner = null;
          try { inner = el.contentDocument; } catch (_) { inner = null; }
          if (inner?.body) walk(inner.body, nextDepth);
          continue;
        }
        walk(el, nextDepth);
      }
    };
    const root = document.body || document.documentElement;
    if (root) walk(root, 0);
    // Drawings have no text: say so where a model reads first.
    const drawings = Array.from(document.querySelectorAll('canvas')).filter((c) => c.width * c.height >= 4096 && !hidden(c)).length;
    return {
      ...(drawings ? { note: `The page has ${drawings} drawing${drawings > 1 ? 's' : ''} whose content only take_screenshot shows.` } : {}),
      url: location.href,
      title: document.title,
      snapshot: lines.join('\n'),
      truncated,
      next_offset: truncated ? index - 1 : null,
      offset: start,
    };
  };
  const viewport = () => ({ width: innerWidth, height: innerHeight, scroll_x: scrollX, scroll_y: scrollY });
  const describe = (id, options = {}) => {
    const el = element(id);
    if (options.scroll !== false) el.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    let rect = el.getBoundingClientRect();
    // An element inside a same-origin frame is placed by its frame.
    let frame = el.ownerDocument.defaultView?.frameElement;
    let x = rect.left, y = rect.top;
    while (frame) {
      const frameRect = frame.getBoundingClientRect();
      x += frameRect.left + frame.clientLeft;
      y += frameRect.top + frame.clientTop;
      frame = frame.ownerDocument.defaultView?.frameElement;
    }
    const center = { x: x + rect.width / 2, y: y + rect.height / 2 };
    let covered = null;
    if (el.ownerDocument === document) {
      const top = document.elementFromPoint(center.x, center.y);
      if (top && top !== el && !el.contains(top) && !top.contains(el)) {
        covered = `${roleOf(top) || top.localName} "${name(top, roleOf(top))}"`;
      }
    }
    return { x, y, width: rect.width, height: rect.height, center, covered, viewport: viewport() };
  };
  const nativeSetter = (el, value) => {
    const proto = el.localName === 'textarea' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
    if (setter) setter.call(el, value); else el.value = value;
  };
  const fire = (el, type) => el.dispatchEvent(new Event(type, { bubbles: true }));
  const fill = (id, value) => {
    let el = element(id);
    // A label stands for its control: models often pass the label's uid.
    if (el.localName === 'label' && el.control) el = el.control;
    el.scrollIntoView({ block: 'center', behavior: 'instant' });
    // An option is chosen in its list, whatever value comes with it.
    if (el.localName === 'option' && el.closest('select')) {
      const list = el.closest('select');
      list.focus();
      el.selected = true;
      fire(list, 'input'); fire(list, 'change');
      return { filled: true, value: el.value };
    }
    if (el.localName === 'select') {
      const wanted = String(value);
      const lower = (s) => text(s).toLowerCase();
      const options = Array.from(el.options);
      // Exact value or label first; then the same words in another case; then
      // the one option whose label the wanted text contains ("Pro plan").
      const containing = options.filter((o) => lower(o.textContent) && lower(wanted).includes(lower(o.textContent)));
      const option = options.find((o) => o.value === wanted) || options.find((o) => text(o.textContent) === text(wanted))
        || options.find((o) => lower(o.textContent) === lower(wanted)) || (containing.length === 1 ? containing[0] : null);
      if (!option) throw new Error(`no_option: no option "${wanted}"; options: ${Array.from(el.options).map((o) => text(o.textContent)).join(', ')}`);
      el.focus();
      option.selected = true;
      fire(el, 'input'); fire(el, 'change');
      return { filled: true, value: option.value };
    }
    if (el.localName === 'input' && (el.type === 'checkbox' || el.type === 'radio')) {
      const want = value === true || String(value).toLowerCase() === 'true';
      if (el.checked !== want) el.click();
      return { filled: true, checked: el.checked };
    }
    if (el.isContentEditable) {
      el.focus();
      el.textContent = String(value);
      el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: String(value) }));
      return { filled: true };
    }
    if (!('value' in el) || !['input', 'textarea'].includes(el.localName)) throw new Error('not_fillable: the element takes no value; use click or type_text');
    if (el.disabled || el.readOnly) throw new Error('not_fillable: the field is disabled or read-only');
    el.focus();
    nativeSetter(el, String(value));
    el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertReplacementText', data: String(value) }));
    fire(el, 'change');
    return { filled: true };
  };
  const focus = (id) => {
    const el = element(id);
    el.scrollIntoView({ block: 'center', behavior: 'instant' });
    el.focus();
    if (typeof el.select === 'function' && el.select && ['text', 'search', 'email', 'url', 'tel', 'password', 'number', ''].includes(el.type ?? '')) {
      try { el.setSelectionRange(el.value.length, el.value.length); } catch (_) { /* not a text field */ }
    }
    return { focused: el === el.ownerDocument.activeElement };
  };
  const scroll = (id, dx, dy) => {
    if (id) {
      element(id).scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    } else {
      scrollBy({ left: dx || 0, top: dy || 0, behavior: 'instant' });
    }
    return viewport();
  };
  const findText = (texts) => {
    const body = document.body?.innerText || '';
    for (const wanted of texts) if (body.includes(wanted)) return wanted;
    return null;
  };
  const markArgs = (ids) => ids.map((id, index) => {
    element(id).setAttribute('data-enzim-agent-arg', String(index));
    return index;
  });
  const state = () => ({ url: location.href, title: document.title, ready: document.readyState, viewport: viewport() });
  // A click on a list option chooses it: the option has no place of its own
  // while its list is closed, and a click there would open the list instead.
  const clickTarget = (id) => {
    const el = element(id);
    if (el.localName === 'option' && el.closest('select')) return { chosen: true, ...fill(id, el.value) };
    return describe(id);
  };
  globalThis.__enzimAgent = { snapshot, describe, clickTarget, fill, focus, scroll, findText, markArgs, state };
  return globalThis.__enzimAgent;
})();
