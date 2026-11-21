(() => {
  if (window.top !== window || globalThis.__enzimResourcePhasesInstalled) return;
  globalThis.__enzimResourcePhasesInstalled = true;
  const origin = performance.timeOrigin;
  const initialURL = location.href;
  const sent = new Set();
  const emit = (stage, now, value) => globalThis.__enzimBenchmarkEmit({
    phase: 'probe', kind: 'resource-phase-audit', origin, initialURL,
    url: location.href, now, stage, ...value
  });
  const snapshot = stage => {
    if (sent.has(stage)) return;
    sent.add(stage);
    const now = performance.now();
    const navigation = performance.getEntriesByType('navigation')[0];
    emit(stage, now, {
      event: 'document', navigation: navigation?.toJSON() ?? null,
      paints: performance.getEntriesByType('paint').map(value => value.toJSON()),
      viewport: [innerWidth, innerHeight],
      identity: {userAgent: navigator.userAgent, platform: navigator.platform},
      metaHttpEquiv: Array.from(document.querySelectorAll('meta[http-equiv]')).slice(0, 64)
        .map(element => element.httpEquiv.slice(0, 128)),
      links: Array.from(document.querySelectorAll('link[href]')).slice(0, 20)
        .map(element => ({href: element.href.slice(0, 1200), rel: element.rel.slice(0, 100),
          as: element.as.slice(0, 50), media: element.media.slice(0, 256),
          urlTruncated: element.href.length > 1200, disabled: element.disabled,
          noncePresent: Boolean(element.nonce)}))
    });
    const entries = performance.getEntriesByType('resource');
    let batch = [];
    let size = 0;
    let sequence = 0;
    const flush = () => {
      if (!batch.length) return;
      emit(stage, now, {event: 'resources', sequence: sequence++, resources: batch});
      batch = [];
      size = 0;
    };
    for (const entry of entries.slice(0, 120)) {
      const resource = entry.toJSON();
      if (resource.name.length > 16000) {
        resource.originalURLLength = resource.name.length;
        resource.name = resource.name.slice(0, 16000);
        resource.urlTruncated = true;
      }
      const bytes = new TextEncoder().encode(JSON.stringify(resource)).byteLength;
      if (size + bytes > 24000) flush();
      batch.push(resource);
      size += bytes;
    }
    flush();
    emit(stage, now, {event: 'complete', resourceCount: Math.min(entries.length, 120),
      totalResourceEntries: entries.length});
  };
  try {
    new PerformanceObserver(list => {
      if (list.getEntries().some(entry => entry.name === 'first-contentful-paint')) {
        snapshot('first-fcp');
      }
    }).observe({type: 'paint', buffered: true});
  } catch (_) {}
  addEventListener('load', () => setTimeout(() => snapshot('load'), 0), {once: true});
  addEventListener('pagehide', () => snapshot('pagehide'), {once: true});
})();
