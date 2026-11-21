// Runs src/credential_bridge.js against a minimal page with one login form
// and checks when it offers saved logins and records a submission.
const fs = require('fs');
const vm = require('vm');
const assert = require('assert');
const path = require('path');
const source = fs.readFileSync(path.join(__dirname, '../src/credential_bridge.js'), 'utf8');

class Node {}
Node.DOCUMENT_POSITION_FOLLOWING = 4;
class Element extends Node {
  matches() { return false; }
  closest() { return null; }
}
class HTMLInputElement extends Element {
  constructor(type, autocomplete, form) {
    super();
    Object.assign(this, {type, autocomplete, form, value: '', name: '', id: '', placeholder: '',
      disabled: false, readOnly: false, hidden: false, maxLength: -1, isConnected: true});
  }
  getAttribute() { return null; }
  getBoundingClientRect() { return {x: 10, y: 10, width: 200, height: 30, left: 10, top: 10, right: 210, bottom: 40}; }
  compareDocumentPosition() { return Node.DOCUMENT_POSITION_FOLLOWING; }
  closest() { return null; }
  getRootNode() { return document; }
}

const listeners = {};
const posted = [];
const form = {action: '', querySelectorAll: () => [username, password]};
const username = new HTMLInputElement('email', 'username', form);
const password = new HTMLInputElement('password', 'current-password', form);
const document = {
  readyState: 'complete', visibilityState: 'visible', activeElement: null,
  hasFocus: () => true,
  querySelectorAll: () => [username, password],
  addEventListener(type, f) { (listeners[type] ||= []).push(f); },
  removeEventListener() {},
};
let now = 1000;
const context = {
  Node, Element, HTMLInputElement, document, WeakRef, WeakMap, Map, Array, Boolean, Number, URL, Uint32Array,
  innerWidth: 1200, innerHeight: 800, isSecureContext: true,
  location: {protocol: 'https:', origin: 'https://login.example', href: 'https://login.example/'},
  crypto: {getRandomValues: a => a.fill(7)},
  performance: {now: () => now},
  getComputedStyle: () => ({visibility: 'visible', display: 'block', opacity: '1'}),
  addEventListener() {}, removeEventListener() {},
};
context.window = context;
context.globalThis = context;
context.window.top = context;
context.window.webkit = {messageHandlers: {enzimCredentials: {postMessage: value => posted.push(JSON.parse(JSON.stringify(value)))}}};
vm.runInNewContext(source, context);

const fire = (type, init = {}) => {
  now += 50;
  const event = {type, isTrusted: true, target: document.activeElement, composedPath: () => [document.activeElement], ...init};
  for (const f of listeners[type] || []) f(event);
};
const offers = () => posted.filter(p => p.kind === 'ready' && p.focus).length;
const submissions = () => posted.filter(p => p.kind === 'submitted').length;

// Clicking the empty password field offers saved logins.
document.activeElement = password;
fire('pointerdown');
fire('click');
assert.strictEqual(offers(), 1, 'a click on the field offers saved logins');

// Typing dismisses the offer; the page refocusing the field must not bring it back.
fire('keydown', {key: 'a'});
password.value = 'a';
fire('input');
assert.ok(posted.some(p => p.kind === 'dismiss'), 'typing dismisses the offer');
password.value = '';
fire('focusin');
assert.strictEqual(offers(), 1, 'a refocus after dismissal stays quiet');
fire('keydown', {key: 'Backspace'});
fire('focusin');
assert.strictEqual(offers(), 1, 'still quiet while the person keeps typing');

// A click on the field asks for it again.
fire('pointerdown');
fire('click');
assert.strictEqual(offers(), 2, 'a click brings the offer back');

// A submit the page makes while the person types is not a sign-in.
password.value = 'secret';
fire('keydown', {key: 't'});
fire('submit');
assert.strictEqual(submissions(), 0, 'a page submit during typing is not recorded');
// Enter is.
now += 400;
fire('keydown', {key: 'Enter', target: password});
assert.strictEqual(submissions(), 1, 'Enter records the sign-in');
now += 400;
fire('pointerdown');
fire('submit');
assert.strictEqual(submissions(), 2, 'a submit after a click is recorded');
console.log('credential bridge: ok');
