(() => {
  const seenClasses = new Set(), seenIds = new Set(), installed = new Set();
  const pending = new Map();
  let timer, style, surveyedUrl, budget = 20000;
  const scan = (node, subtree = true) => {
    if (!(node instanceof Element)) return;
    pending.set(node, subtree || pending.get(node) === true);
    if (pending.size > 2048) pending.clear();
    if (!timer) timer = setTimeout(flush, 100);
  };
  function flush() {
    const metrics = globalThis.__enzimCosmeticMetrics;
    const started = metrics ? performance.now() : 0;
    timer = undefined;
    const classes = [], ids = [];
    let scanned = 0;
    const visit = element => {
      if (++scanned > 6000 || budget <= 0) return;
      for (const name of element.classList) {
        if (name.length > 128 || seenClasses.has(name)) continue;
        seenClasses.add(name); classes.push(name); --budget;
      }
      const id = element.id;
      if (id && id.length <= 128 && !seenIds.has(id)) {
        seenIds.add(id); ids.push(id); --budget;
      }
    };
    for (const [node, subtree] of pending) {
      visit(node);
      if (!subtree) continue;
      const walk = document.createTreeWalker(node, NodeFilter.SHOW_ELEMENT);
      while (scanned <= 6000 && budget > 0 && walk.nextNode()) visit(walk.currentNode);
    }
    pending.clear();
    const survey = classes.length || ids.length || surveyedUrl !== location.href || !style?.isConnected;
    if (survey) surveyedUrl = location.href;
    for (let i = 0; survey && i < Math.max(1, classes.length, ids.length); i += 1024) {
      if (metrics) ++metrics.surveys;
      window.webkit.messageHandlers.enzimCosmetics.postMessage({
        url: location.href, classes: classes.slice(i, i + 1024), ids: ids.slice(i, i + 1024)
      });
    }
    if (metrics) { ++metrics.flushes; metrics.visited += scanned; metrics.ms += performance.now() - started; }
  }
  globalThis.__enzimApplyCosmetics = (url, rules) => {
    if (location.href !== url) return;
    if (!style || !style.isConnected) {
      style = document.createElement('style');
      (document.head || document.documentElement).append(style);
      installed.clear();
    }
    for (const rule of rules) {
      if (installed.has(rule) || installed.size >= 20000) continue;
      try { style.sheet.insertRule(rule, style.sheet.cssRules.length); installed.add(rule); }
      catch (_) {}
    }
  };
  new MutationObserver(records => {
    for (const record of records) {
      if (record.type === 'attributes') scan(record.target, false);
      else for (const node of record.addedNodes) if (node !== style) scan(node);
    }
  }).observe(document, {subtree: true, childList: true, attributes: true, attributeFilter: ['class', 'id']});
  if (document.documentElement) scan(document.documentElement);
  document.addEventListener('DOMContentLoaded', () => scan(document.documentElement), {once: true});
})();
