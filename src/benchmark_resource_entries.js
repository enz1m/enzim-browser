(() => {
  if (window.top !== window) return;
  // One compact row per fetched resource, in chunks below the message limit.
  const row = e => [e.name.slice(0, 120), e.initiatorType, Math.round(e.startTime),
    Math.round(e.domainLookupStart), Math.round(e.connectStart), Math.round(e.secureConnectionStart),
    Math.round(e.connectEnd), Math.round(e.requestStart), Math.round(e.responseStart),
    Math.round(e.responseEnd), e.transferSize, e.decodedBodySize, e.nextHopProtocol];
  const emit = () => {
    const rows = performance.getEntriesByType('resource').map(row);
    for (let index = 0; index < rows.length; index += 40) {
      globalThis.__enzimBenchmarkEmit({kind: 'resource-diagnostic', event: 'entries',
        origin: performance.timeOrigin, url: location.href, index, total: rows.length,
        entries: rows.slice(index, index + 40)});
    }
  };
  addEventListener('load', () => setTimeout(emit, 600), {once: true});
})();
