(() => {
  if (window.top !== window) return;
  const origin = performance.timeOrigin;
  let remaining = 80;
  const emit = detail => {
    if (remaining-- <= 0) return;
    globalThis.__enzimBenchmarkEmit({kind: 'resource-diagnostic', origin,
      url: location.href, now: performance.now(), ...detail});
  };
  const links = () => [...document.querySelectorAll('link[rel="stylesheet"]')]
    .slice(0, 12).map(link => ({url: link.href.slice(0, 180), loaded: !!link.sheet}));
  const bodyObserver = new MutationObserver(() => {
    if (!document.body) return;
    emit({event: 'body-present', stylesheets: links()});
    bodyObserver.disconnect();
  });
  bodyObserver.observe(document, {childList: true, subtree: true});
  try {
    new PerformanceObserver(list => {
      for (const entry of list.getEntries()) {
        if (!['script', 'link', 'css'].includes(entry.initiatorType)) continue;
        emit({event: 'resource', name: entry.name.slice(0, 200),
          initiator: entry.initiatorType, start: entry.startTime,
          end: entry.responseEnd, duration: entry.duration,
          renderBlocking: entry.renderBlockingStatus ?? null,
          decodedBytes: entry.decodedBodySize});
      }
    }).observe({type: 'resource', buffered: true});
  } catch (_) {}
  for (const event of ['DOMContentLoaded', 'load', 'pagehide']) {
    addEventListener(event, () => emit({event, stylesheets: links(),
      fonts: document.fonts?.status}), {once: true});
  }
})();
