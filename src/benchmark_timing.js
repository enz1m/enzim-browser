(() => {
  if (window.top !== window) return;
  if (globalThis.__enzimTimingInstalled) return;
  globalThis.__enzimTimingInstalled = true;
  const origin = performance.timeOrigin;
  const initialURL = location.href;
  const visibility = [{t: 0, state: document.visibilityState}];
  document.addEventListener('visibilitychange', () => {
    visibility.push({t: performance.now(), state: document.visibilityState});
  });
  const emit = phase => {
    const n = performance.getEntriesByType('navigation')[0];
    const paints = performance.getEntriesByType('paint').map(p => ({name: p.name, start: p.startTime}));
    globalThis.__enzimBenchmarkEmit({
      version: 1, phase, origin, initialURL, url: location.href,
      now: performance.now(), visibility: visibility.slice(),
      paints, navigation: n ? JSON.parse(JSON.stringify(n.toJSON())) : null,
      prerendering: document.prerendering === true,
      readyState: document.readyState,
      webdriver: navigator.webdriver,
      viewport: [innerWidth, innerHeight, devicePixelRatio]
    });
  };
  try {
    new PerformanceObserver(() => emit('paint')).observe({type: 'paint', buffered: true});
  } catch (_) {}
  // A page whose load event waits on a slow subresource still shows that its scripts ran.
  document.addEventListener('DOMContentLoaded', () => setTimeout(() => emit('dcl'), 0), {once: true});
  addEventListener('load', () => setTimeout(() => emit('load'), 0), {once: true});
  addEventListener('pagehide', () => emit('pagehide'));
  addEventListener('pageshow', event => {
    if (event.persisted) emit('bfcache-restore');
  });
})();
