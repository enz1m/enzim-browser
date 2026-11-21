/// Passkeys through the real engine: a page calls `navigator.credentials`, the
/// relay reports the engine's origin for it, the authenticator answers, and
/// the page verifies what it got with its own WebCrypto.
#[cfg(feature = "benchmark")]
pub(super) fn run_passkeys_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use crate::credentials::passkey::{self, Incoming, Keeper};
    use enzui_webkit::page::{PageEvent, ScriptFrames, ScriptTiming, ScriptWorld, UserScript};

    /// Runs page-world script and waits for its JSON result.
    fn page(worker: &mut Worker, source: &str) -> Result<String, String> {
        let id = worker
            .session
            .evaluate_script(source, &ScriptWorld::Page)
            .map_err(|_| "The page script did not start".to_owned())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            let mut found = None;
            let mut kept = Vec::new();
            for event in worker.session.take_page_events() {
                match event {
                    PageEvent::ScriptResult { id: done, result } if done == id => {
                        found = Some(result.map_err(|_| "The page script failed".to_owned()));
                    }
                    other => kept.push(other),
                }
            }
            PENDING.with(|pending| pending.borrow_mut().extend(kept));
            if let Some(result) = found {
                return result;
            }
            if std::time::Instant::now() > deadline {
                return Err("The page script timed out".into());
            }
        }
    }
    thread_local! {
        /// Events taken while waiting for a script result, for `request`.
        static PENDING: std::cell::RefCell<Vec<PageEvent>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    /// Waits for the relay's next message and parses it with the origins the
    /// engine recorded.
    fn request(
        worker: &mut Worker,
    ) -> Result<(enzui_webkit::page::FrameDocument, Incoming, String), String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            let mut events = PENDING.with(|pending| std::mem::take(&mut *pending.borrow_mut()));
            events.extend(worker.session.take_page_events());
            for event in events {
                if let PageEvent::FrameSecretMessage {
                    handler,
                    frame,
                    origin,
                    top_origin,
                    main_frame,
                    json,
                } = event
                    && handler == passkey::HANDLER
                {
                    let incoming = Incoming::parse(
                        json.expose_secret(),
                        origin.as_str(),
                        top_origin.as_str(),
                        main_frame,
                    )
                    .ok_or("The relay posted an unreadable message")?;
                    return Ok((frame, incoming, origin.as_str().to_owned()));
                }
            }
            if std::time::Instant::now() > deadline {
                return Err("No passkey request reached the browser".into());
            }
        }
    }
    fn reply(
        worker: &mut Worker,
        frame: &enzui_webkit::page::FrameDocument,
        id: u64,
        result: Result<serde_json::Value, passkey::Failure>,
    ) -> Result<(), String> {
        let mut job = worker
            .session
            .evaluate_secret_script_in_frame(frame, &passkey::reply_script(id, result), passkey::WORLD)
            .map_err(|_| "The reply did not start".to_owned())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            if let Some(result) = job.take() {
                return match result {
                    Ok(value) if value.expose_secret() == "true" => Ok(()),
                    _ => Err("The relay did not take the reply".into()),
                };
            }
            if std::time::Instant::now() > deadline {
                return Err("The reply timed out".into());
            }
        }
    }
    /// Polls the page until `expression` is true.
    fn settled(worker: &mut Worker, expression: &str, label: &str) -> Result<(), String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            if page(worker, expression)? == "true" {
                return Ok(());
            }
            if std::time::Instant::now() > deadline {
                let state = page(worker, "JSON.stringify(globalThis.__gate)").unwrap_or_default();
                return Err(format!("Passkey check failed: {label}: {state}"));
            }
            worker.session.pump(20);
        }
    }

    let mut router = ViewRouter::new();
    let view = router.create(997);
    let mut worker = Worker::open(
        router,
        view,
        scratch,
        Some(enzui_webkit::ViewportSize::from_logical(800., 600.).map_err(|e| e.to_string())?),
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    worker.visible = true;
    if !worker
        .session
        .register_frame_secret_message_handler(passkey::HANDLER, passkey::WORLD)
    {
        return Err("This runtime does not report the origin of a posting frame".into());
    }
    for (source, world) in [
        (passkey::BRIDGE_SOURCE, ScriptWorld::Isolated(passkey::WORLD.into())),
        (passkey::PAGE_SOURCE, ScriptWorld::Page),
    ] {
        worker
            .session
            .add_user_script(&UserScript {
                source: source.into(),
                world,
                timing: ScriptTiming::DocumentStart,
                frames: ScriptFrames::AllFrames,
            })
            .map_err(|error| error.to_string())?;
    }
    worker
        .session
        .load("https://example.com/", 20_000)
        .map_err(|_| "The HTTPS page failed to load".to_owned())?;

    // The page's half: every check a relying party's script could make.
    let fixture = r#"(() => {
        const gate = globalThis.__gate = {};
        const bytes = text => new TextEncoder().encode(text);
        const same = (a, b) => { a = new Uint8Array(a); b = new Uint8Array(b); return a.length === b.length && a.every((v, i) => v === b[i]); };
        const b64 = buffer => btoa(String.fromCharCode(...new Uint8Array(buffer))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
        const raw = der => { // an ASN.1 ECDSA signature as the r||s WebCrypto verifies
            der = new Uint8Array(der);
            let at = 2; const part = () => { at++; const length = der[at++]; let value = der.slice(at, at + length); at += length;
                while (value.length > 32) value = value.slice(1);
                const out = new Uint8Array(32); out.set(value, 32 - value.length); return out; };
            const r = part(), s = part(); const out = new Uint8Array(64); out.set(r); out.set(s, 32); return out; };
        gate.api = typeof PublicKeyCredential === 'function' && typeof navigator.credentials?.create === 'function';
        gate.register = async rp => {
            const challenge = crypto.getRandomValues(new Uint8Array(32));
            const made = await navigator.credentials.create({publicKey: {
                rp: {id: rp, name: 'Gate'}, user: {id: bytes('gate-user'), name: 'gate@example.com', displayName: 'Gate User'},
                challenge, pubKeyCredParams: [{type: 'public-key', alg: -7}], authenticatorSelection: {residentKey: 'required', userVerification: 'required'}}});
            const client = JSON.parse(new TextDecoder().decode(made.response.clientDataJSON));
            gate.key = await crypto.subtle.importKey('spki', made.response.getPublicKey(), {name: 'ECDSA', namedCurve: 'P-256'}, false, ['verify']);
            gate.id = made.rawId;
            const json = made.toJSON();
            return made instanceof PublicKeyCredential && made.type === 'public-key' && made.id === b64(made.rawId)
                && made.response instanceof AuthenticatorAttestationResponse
                && client.type === 'webauthn.create' && client.origin === location.origin && client.challenge === b64(challenge) && client.crossOrigin === false
                && made.response.getPublicKeyAlgorithm() === -7 && made.response.getTransports().includes('internal')
                && made.response.attestationObject.byteLength > made.response.getAuthenticatorData().byteLength
                && json.response.attestationObject === b64(made.response.attestationObject) && made.authenticatorAttachment === 'platform';
        };
        gate.signIn = async allow => {
            const challenge = crypto.getRandomValues(new Uint8Array(32));
            const got = await navigator.credentials.get({publicKey: {challenge, rpId: 'example.com',
                allowCredentials: allow ? [{type: 'public-key', id: gate.id}] : [], userVerification: 'required'}});
            const data = new Uint8Array(got.response.authenticatorData);
            const signed = new Uint8Array(data.length + 32);
            signed.set(data); signed.set(new Uint8Array(await crypto.subtle.digest('SHA-256', got.response.clientDataJSON)), data.length);
            const verified = await crypto.subtle.verify({name: 'ECDSA', hash: 'SHA-256'}, gate.key, raw(got.response.signature), signed);
            const client = JSON.parse(new TextDecoder().decode(got.response.clientDataJSON));
            const rp = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes('example.com')));
            return verified && same(got.rawId, gate.id) && same(got.response.userHandle, bytes('gate-user'))
                && client.type === 'webauthn.get' && client.challenge === b64(challenge) && client.origin === location.origin
                && same(data.slice(0, 32), rp) && (data[32] & 0x05) === 0x05 && got.response instanceof AuthenticatorAssertionResponse;
        };
        gate.run = (name, promise) => { gate[name] = 'pending'; promise.then(value => gate[name] = value, error => gate[name] = error.name); };
        return gate.api && location.origin === 'https://example.com';
    })()"#;
    if page(&mut worker, fixture)? != "true" {
        return Err("The page has no Web Authentication API".into());
    }

    // Registration.
    page(&mut worker, "__gate.run('registered', __gate.register('example.com')); true")?;
    let (frame, incoming, origin) = request(&mut worker)?;
    let Incoming::Request(registration) = incoming else {
        return Err("The registration was not accepted".into());
    };
    // The engine names the effective port; the page's origin does not.
    if origin != "https://example.com:443" || registration.origin != "https://example.com" {
        return Err(format!("The engine reported {origin} for the page"));
    }
    let (stored, created) = registration.create(Keeper::Local)?;
    reply(&mut worker, &frame, registration.id, Ok(created))?;
    settled(&mut worker, "__gate.registered === true", "the page accepts the new credential")?;

    // Sign-in, with and without the relying party naming the credential.
    for (allow, label) in [("true", "named credential"), ("false", "discoverable credential")] {
        page(&mut worker, &format!("__gate.run('signed', __gate.signIn({allow})); true"))?;
        let (frame, incoming, _) = request(&mut worker)?;
        let Incoming::Request(sign_in) = incoming else {
            return Err("The sign-in was not accepted".into());
        };
        if !sign_in.accepts(&stored) {
            return Err(format!("The stored passkey does not answer the request: {label}"));
        }
        reply(&mut worker, &frame, sign_in.id, Ok(sign_in.assert(&stored, Keeper::Local)?))?;
        settled(&mut worker, "__gate.signed === true", label)?;
    }

    // Another site's relying party ID is refused with the name the API gives it.
    page(&mut worker, "__gate.run('foreign', __gate.register('example.org')); true")?;
    let (frame, incoming, _) = request(&mut worker)?;
    let Incoming::Refused(id, failure) = incoming else {
        return Err("A foreign relying party ID was accepted".into());
    };
    reply(&mut worker, &frame, id, Err(failure))?;
    settled(&mut worker, "__gate.foreign === 'SecurityError'", "a foreign relying party is refused")?;

    // A request the user declines, and one the page aborts.
    page(&mut worker, "__gate.run('declined', __gate.signIn(false)); true")?;
    let (frame, incoming, _) = request(&mut worker)?;
    let Incoming::Request(declined) = incoming else {
        return Err("The sign-in was not accepted".into());
    };
    reply(&mut worker, &frame, declined.id, Err(passkey::Failure::NOT_ALLOWED))?;
    settled(&mut worker, "__gate.declined === 'NotAllowedError'", "a declined request rejects")?;
    page(
        &mut worker,
        "(() => { const stop = new AbortController(); __gate.run('aborted', navigator.credentials.get({signal: stop.signal, publicKey: {challenge: new Uint8Array(16)}})); setTimeout(() => stop.abort(), 50); return true; })()",
    )?;
    let (_, first, _) = request(&mut worker)?;
    let Incoming::Request(aborted) = first else {
        return Err("The aborted sign-in was not accepted".into());
    };
    let (_, second, _) = request(&mut worker)?;
    if second != Incoming::Cancel(aborted.id) {
        return Err("An aborted request did not tell the browser".into());
    }
    settled(&mut worker, "__gate.aborted === 'AbortError'", "an aborted request rejects")?;
    Ok(serde_json::json!({
        "registration": true,
        "sign_in": 2,
        "foreign_relying_party_refused": true,
        "declined_and_aborted": true,
        "credential_id_bytes": 16,
    }))
}
