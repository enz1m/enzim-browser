(() => {
  let last, queued;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d', {willReadFrequently: true});
  function report() {
    queued = false;
    if (!context || !document.documentElement) return;
    context.clearRect(0, 0, 1, 1);
    const root = getComputedStyle(document.documentElement).backgroundColor;
    context.fillStyle = root;
    context.fillRect(0, 0, 1, 1);
    if (context.getImageData(0, 0, 1, 1).data[3] === 0 && document.body) {
      context.fillStyle = getComputedStyle(document.body).backgroundColor;
      context.fillRect(0, 0, 1, 1);
    }
    context.globalCompositeOperation = 'destination-over';
    context.fillStyle = '#fff';
    context.fillRect(0, 0, 1, 1);
    context.globalCompositeOperation = 'source-over';
    const color = Array.from(context.getImageData(0, 0, 1, 1).data);
    const value = JSON.stringify({url: location.href, color});
    if (last === value) return;
    last = value;
    window.webkit.messageHandlers.enzimBackground.postMessage({url: location.href, color});
  }
  function schedule() {
    if (!queued) { queued = true; requestAnimationFrame(report); }
  }
  function ready() {
    const observer = new MutationObserver(schedule);
    for (const node of [document.documentElement, document.body]) {
      if (node) observer.observe(node, {attributes: true, attributeFilter: ['class', 'style']});
    }
    schedule();
  }
  if (document.readyState === 'loading') addEventListener('DOMContentLoaded', ready, {once: true});
  else ready();
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', schedule);
  addEventListener('pageshow', schedule);
  addEventListener('load', schedule, {once: true});
})();
