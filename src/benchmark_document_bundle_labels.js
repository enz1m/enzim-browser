(() => {
  if (window.top !== window || location.hostname !== 'www.youtube.com') return;
  const key = '__enzimBenchmarkDocumentBundlesV1';
  const origin = performance.timeOrigin;
  const initialURL = location.href;
  const capture = capturePhase => {
    const started = performance.now();
    const urls = [...new Set([
      ...performance.getEntriesByType('resource').map(r => r.name),
      ...[...document.scripts].map(s => s.src)
    ])].filter(url => typeof url === 'string' && url.length <= 768
      && url.includes('/ytmainappweb/') && new URL(url).origin === location.origin).slice(0, 8);
    const label = {origin, initialURL, url: location.href,
      userAgent: navigator.userAgent, platform: navigator.platform, urls};
    let stored = false;
    try {
      const raw = sessionStorage.getItem(key) || '[]';
      const previous = raw.length <= 98_304 ? JSON.parse(raw) : [];
      const labels = Array.isArray(previous)
        ? previous.filter(label => label?.origin !== origin).slice(-3) : [];
      labels.push(label);
      sessionStorage.setItem(key, JSON.stringify(labels));
      stored = true;
    } catch (_) {}
    globalThis.__enzimBenchmarkEmit({phase: 'probe', kind: 'youtube-document-bundles',
      event: 'document-bundle-label', now: performance.now(), ...label,
      stored, capturePhase, captureMs: performance.now() - started});
  };
  addEventListener('pagehide', () => capture('pagehide'), {once: true});
  try {
    const observer = new PerformanceObserver(entries => {
      if (!entries.getEntries().some(entry => entry.name === 'first-contentful-paint')) return;
      observer.disconnect();
      capture('first-paint');
    });
    observer.observe({type: 'paint', buffered: true});
  } catch (_) {}
})();
