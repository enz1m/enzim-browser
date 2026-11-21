const fs = require('fs'), vm = require('vm'), assert = require('assert');
const root = require('path').resolve(__dirname, '../src') + '/';
const source = fs.readFileSync(root + 'benchmark_document_bundle_labels.js', 'utf8');
const key = '__enzimBenchmarkDocumentBundlesV1';
const url = 'https://www.youtube.com/s/_/ytmainappweb/_/js/k=ytmainappweb.kevlar_base.test.es5.O/m=base';
function run(code, withPagehide) {
  const storage = new Map(), events = [];
  let callback, disconnected = 0;
  for (const origin of [1, 2]) {
    const handlers = {};
    const context = {
      location: {hostname: 'www.youtube.com', origin: 'https://www.youtube.com', href: 'https://www.youtube.com/'},
      performance: {timeOrigin: origin, now: () => 500, getEntriesByType: () => []},
      document: {scripts: [{src: url}]}, navigator: {userAgent: 'fixture', platform: 'fixture'}, URL,
      sessionStorage: {getItem: k => storage.get(k), setItem: (k,v) => storage.set(k,v)},
      addEventListener: (name, handler) => handlers[name] = handler,
      __enzimBenchmarkEmit: value => events.push(value),
      PerformanceObserver: class {
        constructor(handler) { callback = handler; }
        disconnect() { disconnected++; }
        observe(options) { assert.equal(options.type, 'paint'); assert.equal(options.buffered, true); }
      }
    };
    context.window = context; context.top = context;
    vm.runInNewContext(code, context);
    callback({getEntries: () => [{name: 'first-paint'}]});
    assert.equal(events.length, (origin - 1) * (withPagehide ? 2 : 1));
    callback({getEntries: () => [{name: 'first-contentful-paint'}]});
    if (withPagehide) handlers.pagehide();
    const saved = JSON.parse(storage.get(key) || '[]');
    assert.equal(saved.length, origin);
    assert.deepEqual(saved.find(label => label.origin === origin).urls, [url]);
  }
  assert.equal(disconnected, 2);
  assert.equal(events.filter(e => e.capturePhase === 'first-paint').length, 2);
  return storage;
}
run(source, false);
run(source, true);
assert.throws(() => run(source.replace("capture('first-paint');", ''), false));
console.log('Actual first-paint callback retains two document labels without pagehide; pagehide updates rather than duplicates; removing backup fails.');
