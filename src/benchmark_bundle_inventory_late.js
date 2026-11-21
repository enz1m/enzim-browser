(() => {
  if (window.top !== window || location.hostname !== 'www.youtube.com') return;
  const auditAfterCompletion = () => {
    const nav = performance.getEntriesByType('navigation')[0];
    if (document.readyState !== 'complete' || !(nav?.loadEventEnd > 0)
        || !performance.getEntriesByType('paint').some(p => p.name === 'first-contentful-paint')) {
      setTimeout(auditAfterCompletion, 250);
      return;
    }
(() => {
  if (window.top !== window || location.hostname !== 'www.youtube.com') return;
  const origin = performance.timeOrigin;
  const initialURL = location.href;
  const emit = detail => globalThis.__enzimBenchmarkEmit({
    phase: 'probe', kind: 'youtube-bundle-inventory', origin, initialURL,
    url: location.href, now: performance.now(), ...detail
  });
  const identity = {
    userAgent: navigator.userAgent, platform: navigator.platform,
    brands: navigator.userAgentData?.brands ?? null,
    userAgentDataPlatform: navigator.userAgentData?.platform ?? null,
    hardwareConcurrency: navigator.hardwareConcurrency,
    features: {customElements: !!globalThis.customElements,
      webAnimations: typeof Element.prototype.animate === 'function',
      intersectionObserver: !!globalThis.IntersectionObserver,
      weakRef: !!globalThis.WeakRef, temporal: !!globalThis.Temporal}
  };
  emit({event: 'identity', identity});
  let priorDocuments = [];
  try {
    const raw = sessionStorage.getItem('__enzimBenchmarkDocumentBundlesV1') || '[]';
    const stored = raw.length <= 98_304 ? JSON.parse(raw) : [];
    if (Array.isArray(stored)) priorDocuments = stored.filter(label => label && label.origin !== origin && Array.isArray(label.urls)).slice(-4);
  } catch (_) {}
  const priorURLs = priorDocuments.flatMap(label => label.urls.slice(0, 8))
    .filter(url => {
      if (typeof url !== 'string' || url.length > 768 || !url.includes('/ytmainappweb/')) return false;
      try { return new URL(url).origin === location.origin; } catch (_) { return false; }
    });
  emit({event: 'prior-document-labels', priorDocuments});
  const seen = new Set();
  const inventory = async stage => {
    const resources = performance.getEntriesByType('resource')
      .filter(r => r.initiatorType === 'script' || r.name.includes('/ytmainappweb/')).slice(0, 160)
      .map(r => ({url: r.name, start: r.startTime, end: r.responseEnd,
        transferBytes: r.transferSize, encodedBytes: r.encodedBodySize,
        decodedBytes: r.decodedBodySize, nextHopProtocol: r.nextHopProtocol}));
    const scripts = [...document.scripts].filter(s => s.src).slice(0, 80)
      .map(s => ({url: s.src, async: s.async, defer: s.defer, type: s.type}));
    emit({event: 'inventory', stage, resources, scripts});
    const targets = [...new Set([...resources.map(r => r.url), ...scripts.map(s => s.url), ...priorURLs])]
      .filter(url => url.includes('/ytmainappweb/') && new URL(url).origin === location.origin)
      .slice(0, 32);
    for (const url of targets) {
      if (seen.has(url)) continue;
      seen.add(url);
      try {
        const response = await fetch(url, {cache: 'only-if-cached', mode: 'same-origin'});
        if (!response.ok || response.url !== url) throw new Error('Cache response not exact successful resource');
        const bytes = await response.arrayBuffer();
        const hash = await crypto.subtle.digest('SHA-256', bytes);
        const sha256 = [...new Uint8Array(hash)].map(x => x.toString(16).padStart(2, '0')).join('');
        emit({event: 'cached-bundle', url, bytes: bytes.byteLength, sha256,
          cacheMode: 'only-if-cached', contentType: response.headers.get('content-type')});
      } catch (error) {
        emit({event: 'cache-bundle-unavailable', url, error: String(error)});
      }
    }
    emit({event: 'inventory-complete', targetCount: targets.length});
  };
  inventory('after-timing-window');
})();
  };
  setTimeout(auditAfterCompletion, 14000);
})();
