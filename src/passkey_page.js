// The Web Authentication API for pages, in the page's own world. The engine is
// built without it, so this defines the interface and relays each call to the
// browser, which checks the calling origin itself, asks the user and signs.
// Nothing here is trusted by the browser: a page that rewrites it only changes
// what it tells itself.
(() => {
    if (!globalThis.isSecureContext || location.protocol !== 'https:' || globalThis.PublicKeyCredential)
        return;
    const REQUEST = 'enzim-passkey-request';
    const RESPONSE = 'enzim-passkey-response';
    const pending = new Map();
    let sequence = 0;

    const encode = source => {
        const view = source instanceof ArrayBuffer ? new Uint8Array(source)
            : ArrayBuffer.isView(source) ? new Uint8Array(source.buffer, source.byteOffset, source.byteLength)
            : null;
        if (!view) throw new TypeError('Expected an ArrayBuffer or a view of one.');
        let text = '';
        for (const byte of view) text += String.fromCharCode(byte);
        return btoa(text).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    };
    const decode = text => {
        const padded = String(text).replace(/-/g, '+').replace(/_/g, '/');
        const binary = atob(padded + '='.repeat((4 - padded.length % 4) % 4));
        const bytes = new Uint8Array(binary.length);
        for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
        return bytes.buffer;
    };
    const failure = (name, message) => name === 'TypeError' ? new TypeError(message) : new DOMException(message, name);
    const descriptors = list => Array.from(list || [], entry => ({type: String(entry.type), id: encode(entry.id)}));

    addEventListener(RESPONSE, event => {
        let message;
        try { message = typeof event.detail === 'string' ? JSON.parse(event.detail) : null; } catch { return; }
        const call = message && pending.get(message.requestId);
        if (!call) return;
        pending.delete(message.requestId);
        call.settle(message);
    });

    const internal = new WeakMap();
    class AuthenticatorResponse {
        constructor() { throw new TypeError('Illegal constructor'); }
        get clientDataJSON() { return internal.get(this).clientDataJSON; }
    }
    class AuthenticatorAttestationResponse extends AuthenticatorResponse {
        get attestationObject() { return internal.get(this).attestationObject; }
        getTransports() { return internal.get(this).transports.slice(); }
        getAuthenticatorData() { return internal.get(this).authenticatorData; }
        getPublicKey() { return internal.get(this).publicKey; }
        getPublicKeyAlgorithm() { return internal.get(this).publicKeyAlgorithm; }
    }
    class AuthenticatorAssertionResponse extends AuthenticatorResponse {
        get authenticatorData() { return internal.get(this).authenticatorData; }
        get signature() { return internal.get(this).signature; }
        get userHandle() { return internal.get(this).userHandle; }
    }
    class PublicKeyCredential {
        constructor() { throw new TypeError('Illegal constructor'); }
        get id() { return internal.get(this).id; }
        get rawId() { return internal.get(this).rawId; }
        get type() { return 'public-key'; }
        get response() { return internal.get(this).response; }
        get authenticatorAttachment() { return 'platform'; }
        getClientExtensionResults() { return internal.get(this).extensions; }
        toJSON() {
            const state = internal.get(this);
            const response = internal.get(state.response);
            const json = {clientDataJSON: encode(response.clientDataJSON)};
            if (state.response instanceof AuthenticatorAttestationResponse) {
                Object.assign(json, {
                    attestationObject: encode(response.attestationObject),
                    authenticatorData: encode(response.authenticatorData),
                    transports: response.transports.slice(),
                    publicKey: encode(response.publicKey),
                    publicKeyAlgorithm: response.publicKeyAlgorithm,
                });
            } else {
                Object.assign(json, {
                    authenticatorData: encode(response.authenticatorData),
                    signature: encode(response.signature),
                });
                if (response.userHandle) json.userHandle = encode(response.userHandle);
            }
            return {
                id: state.id, rawId: state.id, type: 'public-key', response: json,
                authenticatorAttachment: 'platform', clientExtensionResults: state.extensions,
            };
        }
        static isUserVerifyingPlatformAuthenticatorAvailable() { return Promise.resolve(true); }
        static isConditionalMediationAvailable() { return Promise.resolve(false); }
        static getClientCapabilities() {
            return Promise.resolve({
                conditionalCreate: false, conditionalGet: false, hybridTransport: false,
                passkeyPlatformAuthenticator: true, userVerifyingPlatformAuthenticator: true,
                relatedOrigins: false, signalAllAcceptedCredentials: false,
                signalCurrentUserDetails: false, signalUnknownCredential: false,
            });
        }
        static parseCreationOptionsFromJSON(options) {
            const parsed = {...options, challenge: decode(options.challenge)};
            parsed.user = {...options.user, id: decode(options.user.id)};
            if (options.excludeCredentials)
                parsed.excludeCredentials = options.excludeCredentials.map(entry => ({...entry, id: decode(entry.id)}));
            return parsed;
        }
        static parseRequestOptionsFromJSON(options) {
            const parsed = {...options, challenge: decode(options.challenge)};
            if (options.allowCredentials)
                parsed.allowCredentials = options.allowCredentials.map(entry => ({...entry, id: decode(entry.id)}));
            return parsed;
        }
    }
    const make = (prototype, state) => {
        const object = Object.create(prototype);
        internal.set(object, state);
        return object;
    };
    const credential = (kind, message, extensions) => {
        const response = kind === 'create'
            ? make(AuthenticatorAttestationResponse.prototype, {
                clientDataJSON: decode(message.clientDataJSON),
                attestationObject: decode(message.attestationObject),
                authenticatorData: decode(message.authenticatorData),
                publicKey: decode(message.publicKey),
                publicKeyAlgorithm: message.publicKeyAlgorithm,
                transports: message.transports || ['internal'],
            })
            : make(AuthenticatorAssertionResponse.prototype, {
                clientDataJSON: decode(message.clientDataJSON),
                authenticatorData: decode(message.authenticatorData),
                signature: decode(message.signature),
                userHandle: message.userHandle ? decode(message.userHandle) : null,
            });
        return make(PublicKeyCredential.prototype, {id: message.id, rawId: decode(message.id), response, extensions});
    };

    const call = (kind, options, signal, extensions) => new Promise((resolve, reject) => {
        if (signal?.aborted) {
            reject(signal.reason ?? failure('AbortError', 'The operation was aborted.'));
            return;
        }
        const id = ++sequence;
        const send = detail => dispatchEvent(new CustomEvent(REQUEST, {detail: JSON.stringify(detail)}));
        const abort = () => {
            if (!pending.delete(id)) return;
            send({kind: 'cancel', id});
            reject(signal.reason ?? failure('AbortError', 'The operation was aborted.'));
        };
        signal?.addEventListener('abort', abort, {once: true});
        pending.set(id, {settle(message) {
            signal?.removeEventListener('abort', abort);
            if (message.ok) resolve(credential(kind, message, extensions));
            else reject(failure(message.name, message.message));
        }});
        send({kind, id, options});
    });

    const create = async options => {
        const key = options?.publicKey;
        if (!key) throw failure('NotSupportedError', 'Only public key credentials can be created.');
        if (!key.rp || !key.user || !key.challenge || !key.pubKeyCredParams)
            throw new TypeError('The public key credential creation options are incomplete.');
        const extensions = key.extensions?.credProps ? {credProps: {rk: true}} : {};
        return call('create', {
            rp: {id: key.rp.id == null ? null : String(key.rp.id), name: String(key.rp.name ?? '')},
            user: {id: encode(key.user.id), name: String(key.user.name ?? ''), displayName: String(key.user.displayName ?? '')},
            challenge: encode(key.challenge),
            pubKeyCredParams: Array.from(key.pubKeyCredParams, entry => ({type: String(entry.type), alg: Number(entry.alg)})),
            excludeCredentials: descriptors(key.excludeCredentials),
        }, options.signal, extensions);
    };
    const get = async options => {
        const key = options?.publicKey;
        if (!key) return null;
        if (!key.challenge) throw new TypeError('The public key credential request options are incomplete.');
        if (options.mediation === 'conditional') {
            // No autofill surface offers passkeys: the request stays pending until the page aborts it.
            return new Promise((_, reject) => options.signal?.addEventListener('abort',
                () => reject(options.signal.reason ?? failure('AbortError', 'The operation was aborted.')), {once: true}));
        }
        return call('get', {
            rpId: key.rpId == null ? null : String(key.rpId),
            challenge: encode(key.challenge),
            allowCredentials: descriptors(key.allowCredentials),
        }, options.signal, {});
    };
    const container = Object.freeze({
        create, get,
        store: () => Promise.reject(failure('NotSupportedError', 'Storing credentials is not supported.')),
        preventSilentAccess: () => Promise.resolve(),
    });
    for (const [name, value] of Object.entries({
        PublicKeyCredential, AuthenticatorResponse, AuthenticatorAttestationResponse, AuthenticatorAssertionResponse,
    }))
        Object.defineProperty(globalThis, name, {value, writable: true, configurable: true, enumerable: false});
    Object.defineProperty(Navigator.prototype, 'credentials', {get: () => container, configurable: true, enumerable: true});
})();
