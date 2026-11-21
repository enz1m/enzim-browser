const fs = require('fs');
const vm = require('vm');
const assert = require('assert');
const path = require('path');
const source = fs.readFileSync(path.join(__dirname, '../src/benchmark_resource_phases.js'), 'utf8');
function exercise(code) {
  const events = [];
  const callbacks = {};
  let observer;
  const resource = {name: 'https://www.youtube.com/main.js', startTime: 21,
    domainLookupStart: 22, domainLookupEnd: 23, connectStart: 24,
    secureConnectionStart: 25, connectEnd: 26, requestStart: 27,
    responseStart: 33, responseEnd: 44, encodedBodySize: 1234,
    decodedBodySize: 5678, transferSize: 1534, nextHopProtocol: 'h2'};
  const resources = Array.from({length: 125}, (_, i) => ({...resource,
    name: i === 0 ? resource.name : 'https://www.youtube.com/' + 'x'.repeat(i === 1 ? 65000 : 4000),
    toJSON() { const {toJSON, ...rest} = this; return rest; }}));
  const context = {TextEncoder, Set, JSON, Array, Boolean, innerWidth: 1380,
    innerHeight: 797, location: {href: 'https://www.youtube.com/'},
    navigator: {userAgent: 'TestAgent', platform: 'Linux'},
    document: {querySelectorAll(selector) { return selector.startsWith('meta') ?
      [{httpEquiv: 'origin-trial'}, {httpEquiv: 'content-security-policy'}] :
      [{href: 'https://www.youtube.com/main.css', rel: 'stylesheet', as: '', media: '', disabled: false, nonce: 'test'}]; }},
    performance: {timeOrigin: 123456, now: () => 201,
      getEntriesByType(type) { return type === 'resource' ? resources : type === 'paint' ?
        [{name: 'first-contentful-paint', startTime: 200, toJSON() {return {name: this.name, startTime: this.startTime};}}] :
        [{toJSON: () => ({responseStart: 10, responseEnd: 20, loadEventEnd: 0})}]; }},
    __enzimBenchmarkEmit: event => events.push(event),
    PerformanceObserver: class {constructor(callback) { observer = callback; } observe() {}},
    addEventListener: (name, callback) => callbacks[name] = callback,
    setTimeout: callback => callback()};
  context.window = context;
  context.top = context;
  vm.runInNewContext(code, context);
  assert.equal(events.length, 0, 'Snapshot executed before a timing lifecycle event');
  observer({getEntries: () => [{name: 'first-contentful-paint'}]});
  callbacks.load();
  callbacks.pagehide();
  const docs = events.filter(event => event.event === 'document');
  assert.deepEqual(docs.map(event => event.stage), ['first-fcp', 'load', 'pagehide']);
  assert.deepEqual(Array.from(docs[0].metaHttpEquiv), ['origin-trial', 'content-security-policy']);
  assert.equal(docs[0].links[0].noncePresent, true);
  for (const stage of ['first-fcp', 'load', 'pagehide']) {
    const found = events.filter(event => event.stage === stage && event.event === 'resources').flatMap(event => event.resources);
    assert.equal(found.length, 120);
    for (const key of ['connectStart', 'requestStart', 'responseStart', 'responseEnd', 'encodedBodySize', 'nextHopProtocol']) assert.equal(found[0][key], resource[key]);
    assert.equal(found[1].originalURLLength, 65024);
    assert.equal(found[1].urlTruncated, true);
    assert.equal(found[1].name.length, 16000);
    const complete = events.find(event => event.stage === stage && event.event === 'complete');
    assert.equal(complete.totalResourceEntries, 125);
  }
  assert(events.every(event => Buffer.byteLength(JSON.stringify(event)) < 65536));
}
exercise(source);
for (const fault of [source.replace("  try {\n", "  snapshot('early');\n  try {\n"),
                     source.replace('const resource = entry.toJSON();', 'const resource = entry.toJSON(); resource.connectStart = 0;')]) {
  assert.throws(() => exercise(fault));
}
process.stdout.write('Lifecycle/first-document snapshots, exact raw connection timings, bounded chunks and truncation provenance pass; early snapshot and rewritten timing faults rejected. No network requests.\n');
