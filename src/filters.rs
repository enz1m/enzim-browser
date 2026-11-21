use std::{
    borrow::Cow,
    sync::{Arc, OnceLock},
};
pub(crate) type Source = Cow<'static, str>;
fn release_filter_build_memory() {
    #[cfg(all(feature = "wpe", not(test)))]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static PENDING: AtomicBool = AtomicBool::new(false);
        if PENDING.swap(true, Ordering::AcqRel) {
            return;
        }
        if std::thread::Builder::new()
            .name("enzim-filter-memory".into())
            .spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(1));
                #[cfg(feature = "benchmark")]
                let started = std::time::Instant::now();
                let released = enzui_webkit::ffi::runtime::release_host_allocator_memory();
                #[cfg(feature = "benchmark")]
                if std::env::var_os("ENZIM_MEMORY_TRACE").is_some_and(|value| value == "1") {
                    eprintln!(
                        "HOST_MEMORY_RELEASE {}",
                        serde_json::json!({
                            "elapsed_us": started.elapsed().as_micros(), "released": released
                        })
                    );
                }
                #[cfg(not(feature = "benchmark"))]
                let _ = released;
                PENDING.store(false, Ordering::Release);
            })
            .is_err()
        {
            PENDING.store(false, Ordering::Release);
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Compiled {
    pub(crate) json: Arc<Source>,
    bundled: bool,
    #[cfg(feature = "benchmark")]
    pub(crate) converted: usize,
    #[cfg(feature = "benchmark")]
    pub(crate) input: usize,
    #[cfg(feature = "benchmark")]
    pub(crate) rules: usize,
}
static CACHES: [OnceLock<Result<Compiled, String>>; 12] = [const { OnceLock::new() }; 12];

pub(crate) fn compiled(mode: usize) -> Result<Compiled, String> {
    CACHES
        .get(mode)
        .ok_or_else(|| "Unsupported protection mode".to_owned())?
        .get_or_init(|| convert(mode))
        .clone()
}
#[cfg(test)]
use crate::filter_rules::native_rules_json;
use crate::filter_rules::sources;

pub(crate) fn cold_preparation_enabled() -> bool {
    #[cfg(feature = "benchmark")]
    {
        std::env::var_os("ENZIM_BENCH_COLD_FILTER_PREPARE").is_some_and(|value| value == "1")
    }
    #[cfg(not(feature = "benchmark"))]
    {
        false
    }
}

pub(crate) fn bundled_for_privacy(
    privacy: &crate::privacy::Privacy,
) -> Option<(usize, Vec<Arc<Source>>)> {
    if !privacy.site_settings.is_empty() || !privacy.allowed_sites.is_empty() {
        return None;
    }
    #[cfg(feature = "benchmark")]
    if std::env::var_os("ENZIM_BENCH_NATIVE_COSMETICS").is_some_and(|value| value == "0") {
        return None;
    }
    for_privacy(privacy).ok().filter(|(_, sources)| {
        sources
            .iter()
            .all(|source| matches!(&**source, Cow::Borrowed(_)))
    })
}

pub(crate) fn cached_for_privacy(
    privacy: &crate::privacy::Privacy,
) -> Option<(usize, Vec<Arc<Source>>)> {
    if !privacy.site_settings.is_empty() || !privacy.allowed_sites.is_empty() {
        return None;
    }
    for (key, category) in [("ads", 8), ("trackers", 4)] {
        if privacy.value(key) {
            let _ = CACHES.get(privacy.blocking.clamp(1, 2) | category)?.get()?;
        }
    }
    for_privacy(privacy).ok()
}

pub(crate) fn for_privacy(
    privacy: &crate::privacy::Privacy,
) -> Result<(usize, Vec<Arc<Source>>), String> {
    let mut bundle = Vec::new();
    for (key, category) in [("ads", 8), ("trackers", 4)] {
        let enabled = privacy.value(key);
        let mut exceptions = Vec::new();
        for origin in privacy
            .site_settings
            .keys()
            .chain(privacy.allowed_sites.iter())
        {
            if privacy.value_for(key, origin) != enabled {
                let escaped = origin
                    .chars()
                    .flat_map(|c| {
                        if c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_') {
                            vec![c]
                        } else {
                            vec!['\\', c]
                        }
                    })
                    .collect::<String>();
                let pattern = format!("^{escaped}/");
                if !exceptions.contains(&pattern) {
                    exceptions.push(pattern);
                }
            }
        }
        if !enabled && exceptions.is_empty() {
            continue;
        }
        let mode = privacy.blocking.clamp(1, 2) | category;
        let source = compiled(mode)?.json;
        if exceptions.is_empty() {
            bundle.push(source);
            continue;
        }
        let mut rules: Vec<serde_json::Value> =
            serde_json::from_str(&source).map_err(|e| e.to_string())?;
        let mut exception = serde_json::json!({"trigger":{"url-filter":".*"},"action":{"type":"ignore-previous-rules"}});
        exception["trigger"][if enabled {
            "if-top-url"
        } else {
            "unless-top-url"
        }] = serde_json::json!(exceptions);
        rules.push(exception);
        bundle.push(Arc::new(
            serde_json::to_string(&rules)
                .map_err(|e| e.to_string())?
                .into(),
        ));
    }
    Ok((
        if bundle.is_empty() {
            0
        } else {
            privacy.blocking.clamp(1, 2)
        },
        bundle,
    ))
}
include!(concat!(env!("OUT_DIR"), "/bundled_filters.rs"));

#[cfg(feature = "wpe")]
pub(crate) fn native_bytecode(json: &str) -> Option<&'static [u8]> {
    [5, 6, 9, 10].into_iter().find_map(|mode| {
        let (source, bytecode, _, _, _) = bundled(mode)?;
        ((std::ptr::eq(source.as_ptr(), json.as_ptr()) && source.len() == json.len()
            || source == json)
            && !bytecode.is_empty())
        .then_some(bytecode)
    })
}

fn convert(mode: usize) -> Result<Compiled, String> {
    let native_cosmetics = {
        #[cfg(feature = "benchmark")]
        {
            std::env::var_os("ENZIM_BENCH_NATIVE_COSMETICS").is_none_or(|value| value != "0")
        }
        #[cfg(not(feature = "benchmark"))]
        {
            true
        }
    };
    if native_cosmetics && let Some((json, _, _input, _converted, _rules)) = bundled(mode) {
        return Ok(Compiled {
            json: Arc::new(Cow::Borrowed(json)),
            bundled: true,
            #[cfg(feature = "benchmark")]
            input: _input,
            #[cfg(feature = "benchmark")]
            converted: _converted,
            #[cfg(feature = "benchmark")]
            rules: _rules,
        });
    }
    let bundle = crate::filter_rules::convert(mode, native_cosmetics)?;
    let _ = (bundle.input, bundle.converted, bundle.rules);
    let result = Compiled {
        json: Arc::new(Cow::Owned(bundle.json)),
        bundled: false,
        #[cfg(feature = "benchmark")]
        input: bundle.input,
        #[cfg(feature = "benchmark")]
        converted: bundle.converted,
        #[cfg(feature = "benchmark")]
        rules: bundle.rules,
    };
    release_filter_build_memory();
    Ok(result)
}

#[cfg(test)]
fn add_cosmetic_list(
    set: &mut adblock::lists::FilterSet,
    source: &str,
    options: adblock::lists::ParseOptions,
) {
    crate::cosmetic_engine::add_list(set, source, options, false);
}

#[cfg(feature = "benchmark")]
fn full_filter_engines() -> bool {
    std::env::var_os("ENZIM_BENCH_FULL_FILTER_ENGINES").is_some_and(|value| value == "1")
}

fn load_cosmetic_data(data: &[u8]) -> Option<adblock::Engine> {
    let mut engine = adblock::Engine::default();
    engine.deserialize(data).ok()?;
    Some(engine)
}

fn install_cosmetic_resources(engine: &mut adblock::Engine) {
    if let Ok(resources) = serde_json::from_str::<Vec<adblock::resources::Resource>>(include_str!(
        "../resources/filters/scriptlets.json"
    )) {
        engine.use_resources(resources);
    }
}

fn cosmetic_engine() -> adblock::Engine {
    #[cfg(feature = "benchmark")]
    let full_network = full_filter_engines();
    #[cfg(not(feature = "benchmark"))]
    let full_network = false;
    #[cfg(feature = "benchmark")]
    let prepared = !full_network
        && std::env::var_os("ENZIM_BENCH_PARSE_COSMETICS").is_none_or(|value| value != "1");
    #[cfg(not(feature = "benchmark"))]
    let prepared = true;
    let mut engine = if prepared {
        load_cosmetic_data(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/cosmetic-engine.bin"
        )))
    } else {
        None
    }
    .unwrap_or_else(|| crate::cosmetic_engine::build(full_network));
    install_cosmetic_resources(&mut engine);
    release_filter_build_memory();
    engine
}

pub(crate) fn known_identifier(
    source: &Arc<Source>,
) -> Option<enzui_webkit::content_filter::ContentFilterId> {
    #[cfg(feature = "benchmark")]
    if std::env::var_os("ENZIM_BENCH_NO_BUNDLED_IDENTITIES").is_some_and(|value| value == "1") {
        return None;
    }
    [5, 6, 9, 10].into_iter().find_map(|mode| {
        let compiled = CACHES.get(mode)?.get()?.as_ref().ok()?;
        if compiled.bundled && Arc::ptr_eq(source, &compiled.json) {
            enzui_webkit::content_filter::ContentFilterId::parse(bundled_identity(mode)?).ok()
        } else {
            None
        }
    })
}
struct CachedCosmetics {
    address: String,
    resources: adblock::cosmetic_filter_cache::UrlSpecificResources,
    rules: Vec<String>,
    actions: Vec<String>,
}
thread_local! {
    static COSMETICS: adblock::Engine = cosmetic_engine();
    static COSMETIC_URLS: std::cell::RefCell<std::collections::VecDeque<CachedCosmetics>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}
pub(crate) fn prepare_cosmetic_engine() {
    COSMETICS.with(|_| {});
}

fn with_cosmetics<T>(
    address: &str,
    apply: impl FnOnce(&adblock::Engine, &CachedCosmetics) -> T,
) -> T {
    COSMETICS.with(|engine| {
        COSMETIC_URLS.with(|cache| {
            let mut cache = cache.borrow_mut();
            let index = cache.iter().position(|entry| entry.address == address);
            #[cfg(feature = "benchmark")]
            let index = if std::env::var_os("ENZIM_BENCH_NO_COSMETIC_CACHE").is_some() {
                None
            } else {
                index
            };
            if let Some(index) = index {
                if let Some(entry) = cache.remove(index) {
                    cache.push_front(entry);
                }
            } else {
                let resources = engine.url_cosmetic_resources(address);
                let mut rules: Vec<_> = resources
                    .hide_selectors
                    .iter()
                    .map(|selector| format!("{selector} {{ display: none !important; }}"))
                    .collect();
                rules.sort();
                let mut actions: Vec<_> = resources.procedural_actions.iter().collect();
                actions.sort();
                let actions = actions
                    .into_iter()
                    .filter_map(|action| {
                        if let Ok(action) = serde_json::from_str::<
                            adblock::cosmetic_filter_cache::ProceduralOrActionFilter,
                        >(action)
                            && let Some((selector, style)) = action.as_css()
                        {
                            Some(format!("{selector} {{ {style} }}"))
                        } else {
                            None
                        }
                    })
                    .collect();
                cache.push_front(CachedCosmetics {
                    address: address.to_owned(),
                    resources,
                    rules,
                    actions,
                });
                cache.truncate(4);
            }
            apply(engine, &cache[0])
        })
    })
}
pub(crate) fn scriptlets(address: &str) -> String {
    with_cosmetics(address, |_, cache| cache.resources.injected_script.clone())
}
pub(crate) fn cosmetic_rules(address: &str, classes: &[String], ids: &[String]) -> Vec<String> {
    with_cosmetics(address, |engine, cache| {
        let mut rules = cache.rules.clone();
        if !cache.resources.generichide {
            rules.extend(
                engine
                    .hidden_class_id_selectors(classes, ids, &cache.resources.exceptions)
                    .into_iter()
                    .map(|selector| format!("{selector} {{ display: none !important; }}")),
            );
        }
        rules.sort();
        rules.dedup();
        rules.extend(cache.actions.iter().cloned());
        rules
    })
}

pub(crate) fn allows_image(address: &str, source: &str, mode: usize) -> bool {
    if mode == 0 {
        return true;
    }
    let Ok(request) = adblock::request::Request::new(address, source, "image", "GET") else {
        return false;
    };
    if mode & 3 == 1 && !request.is_third_party {
        return true;
    }
    thread_local! {static NETWORKS: std::cell::RefCell<std::collections::BTreeMap<usize,adblock::Engine>>=const {std::cell::RefCell::new(std::collections::BTreeMap::new())};}
    NETWORKS.with(|engines| {
        let mut engines = engines.borrow_mut();
        let engine = engines.entry(mode & 12).or_insert_with(|| {
            let mut set = adblock::lists::FilterSet::new(false);
            for source in sources(mode) {
                let rule_types = adblock::lists::RuleTypes::NetworkOnly;
                #[cfg(feature = "benchmark")]
                let rule_types = if full_filter_engines() {
                    adblock::lists::RuleTypes::All
                } else {
                    rule_types
                };
                set.add_filter_list(
                    source.into(),
                    adblock::lists::ParseOptions {
                        rule_types,
                        ..Default::default()
                    },
                );
            }
            let engine = adblock::Engine::new_with_filter_set(set);
            release_filter_build_memory();
            engine
        });
        !engine.check_network_request(&request).should_block()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scriptlet_units(script: &str) -> (Vec<String>, Vec<String>) {
        use base64::Engine as _;
        let resources: Vec<adblock::resources::Resource> =
            serde_json::from_str(include_str!("../resources/filters/scriptlets.json")).unwrap();
        let mut templates: Vec<_> = resources
            .into_iter()
            .filter_map(|resource| {
                base64::engine::general_purpose::STANDARD
                    .decode(resource.content)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .map(|content| format!("{content}\n"))
            })
            .collect();
        templates.sort_by_key(|text| std::cmp::Reverse(text.len()));
        let mut remaining = script;
        let mut dependencies = Vec::new();
        while let Some(template) = templates
            .iter()
            .find(|template| remaining.starts_with(template.as_str()))
        {
            dependencies.push(template.clone());
            remaining = remaining
                .strip_prefix(template.as_str())
                .expect("matched scriptlet resource");
        }
        // adblock does not guarantee dependency or invocation order; compare complete emitted units.
        let mut invocations: Vec<_> = remaining
            .split("\n} catch ( e ) { }\n")
            .filter(|part| !part.is_empty())
            .map(|part| {
                part.strip_prefix("try {\n")
                    .expect("scriptlet invocation wrapper")
                    .to_owned()
            })
            .collect();
        dependencies.sort();
        invocations.sort();
        (dependencies, invocations)
    }

    #[test]
    fn prepared_cosmetic_data_preserves_rules_exceptions_and_scriptlet_permissions() {
        prepare_cosmetic_engine();
        let mut prepared = load_cosmetic_data(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/cosmetic-engine.bin"
        )))
        .expect("prepared cosmetic data");
        let mut reference = crate::cosmetic_engine::build(false);
        install_cosmetic_resources(&mut prepared);
        install_cosmetic_resources(&mut reference);
        let classes = ["ADBAR", "ad-container", "advertisement", "video-ads"].map(str::to_owned);
        let ids = ["ad", "ads", "sponsored"].map(str::to_owned);
        for address in [
            "https://www.google.com/",
            "https://www.youtube.com/",
            "https://edition.cnn.com/",
            "https://en.wikipedia.org/wiki/Main_Page",
            "https://www.bbc.com/news",
            "https://duckduckgo.com/",
            "https://example.com/",
            "https://shop.example/path",
        ] {
            let want = reference.url_cosmetic_resources(address);
            let got = prepared.url_cosmetic_resources(address);
            with_cosmetics(address, |_, warmed| {
                assert_eq!(warmed.resources.generichide, want.generichide);
                assert_eq!(warmed.resources.hide_selectors, want.hide_selectors);
                assert_eq!(warmed.resources.exceptions, want.exceptions);
                assert_eq!(warmed.resources.procedural_actions, want.procedural_actions);
                assert!(
                    scriptlet_units(&warmed.resources.injected_script)
                        == scriptlet_units(&want.injected_script),
                    "warmed scriptlet dependency, argument or permission mismatch at {address}"
                );
            });
            assert_eq!(
                got.generichide, want.generichide,
                "generic-hide policy at {address}"
            );
            assert_eq!(
                got.hide_selectors, want.hide_selectors,
                "selectors at {address}"
            );
            assert_eq!(
                got.exceptions, want.exceptions,
                "selector exceptions at {address}"
            );
            assert_eq!(
                got.procedural_actions, want.procedural_actions,
                "procedural actions at {address}"
            );
            assert!(
                scriptlet_units(&got.injected_script) == scriptlet_units(&want.injected_script),
                "scriptlet dependency, argument or permission mismatch at {address}"
            );
            assert_eq!(
                prepared.hidden_class_id_selectors(&classes, &ids, &got.exceptions),
                reference.hidden_class_id_selectors(&classes, &ids, &want.exceptions),
                "class/ID selectors at {address}"
            );
        }
    }

    #[test]
    fn prepared_cosmetic_data_rejects_corruption_and_truncation() {
        let data = include_bytes!(concat!(env!("OUT_DIR"), "/cosmetic-engine.bin"));
        let mut corrupted = data.to_vec();
        let index = corrupted.len() / 2;
        corrupted[index] ^= 1;
        assert!(
            load_cosmetic_data(&corrupted).is_none(),
            "corrupt prepared rules must use the parser fallback"
        );
        assert!(
            load_cosmetic_data(&data[..data.len() / 2]).is_none(),
            "truncated rules must use the parser fallback"
        );
    }

    #[test]
    fn bundled_preparation_borrows_sources_and_keeps_mutations_owned() {
        let privacy = crate::privacy::Privacy::default();
        let (_, sources) = bundled_for_privacy(&privacy).expect("bundled policy");
        for source in sources {
            assert!(
                matches!(&*source, Cow::Borrowed(_)),
                "bundled rules must not allocate JSON copies"
            );
            let mut edited = Arc::clone(&source);
            Arc::make_mut(&mut edited).to_mut().push(' ');
            assert!(
                matches!(&*edited, Cow::Owned(_)),
                "edited policy must own its changed bytes"
            );
            assert!(
                !Arc::ptr_eq(&source, &edited),
                "edits cannot alter the cached allocation"
            );
            assert!(
                known_identifier(&edited).is_none(),
                "edited policy needs ordinary identity validation"
            );
        }
        let mut overridden = privacy;
        overridden.allowed_sites.push("https://example.com".into());
        assert!(
            bundled_for_privacy(&overridden).is_none(),
            "site exceptions cannot use the bundled preparation path"
        );
    }

    #[test]
    fn prepared_native_identities_apply_only_to_unchanged_bundled_allocations() {
        use enzui_webkit::ffi::content_filters::ContentFilterStore;
        for mode in [5, 6, 9, 10] {
            let original = compiled(mode).expect("bundled rules").json;
            let expected = ContentFilterStore::identifier_for(&original).expect("SDK identity");
            assert_eq!(
                known_identifier(&original),
                Some(expected),
                "prepared identity for mode {mode}"
            );
            let replacement = Arc::new(Cow::Owned(original.to_string()));
            assert!(
                known_identifier(&replacement).is_none(),
                "an independent allocation needs ordinary validation"
            );
            let mut modified = Arc::clone(&original);
            Arc::make_mut(&mut modified).to_mut().push(' ');
            assert!(
                known_identifier(&modified).is_none(),
                "modified policy cannot inherit the bundled identity"
            );
            assert_ne!(
                ContentFilterStore::identifier_for(&modified).unwrap(),
                known_identifier(&original).unwrap()
            );
        }
    }
    #[test]
    fn cached_preparation_reuses_the_compiled_policy_sources() {
        let privacy = crate::privacy::Privacy::default();
        let expected = for_privacy(&privacy).expect("compiled policy");
        let cached = cached_for_privacy(&privacy).expect("cached policy");
        assert_eq!(cached.0, expected.0);
        assert_eq!(cached.1.len(), expected.1.len());
        assert!(
            cached
                .1
                .iter()
                .zip(&expected.1)
                .all(|(a, b)| Arc::ptr_eq(a, b)),
            "preparation must share source allocations"
        );
    }

    #[test]
    fn cached_preparation_leaves_site_exceptions_to_navigation_preparation() {
        let mut privacy = crate::privacy::Privacy::default();
        for_privacy(&privacy).expect("compiled defaults");
        privacy.allowed_sites.push("https://example.com".into());
        assert!(
            cached_for_privacy(&privacy).is_none(),
            "site exceptions must not be rebuilt on the UI thread"
        );
        privacy.allowed_sites.clear();
        privacy.site_settings.insert(
            "https://example.com".into(),
            std::collections::BTreeMap::from([("ads".into(), false)]),
        );
        assert!(
            cached_for_privacy(&privacy).is_none(),
            "site settings must retain asynchronous preparation"
        );
    }

    #[cfg(feature = "wpe")]
    #[test]
    fn bundled_rules_match_current_sources_and_reject_modified_policy() {
        for mode in [5, 6, 9, 10] {
            let fresh = crate::filter_rules::convert(mode, true).expect("current filter source");
            let prepared = compiled(mode).expect("bundled filter source");
            assert_eq!(*prepared.json, fresh.json);
            assert!(native_bytecode(&fresh.json).is_some_and(|bytes| !bytes.is_empty()));
            let rules: serde_json::Value = serde_json::from_str(&fresh.json).expect("rule list");
            assert!(
                rules
                    .as_array()
                    .expect("rules")
                    .iter()
                    .any(|rule| rule["action"]["type"] == "block")
            );
            if mode & 4 == 0 {
                assert!(
                    rules
                        .as_array()
                        .expect("rules")
                        .iter()
                        .any(|rule| rule["action"]["type"] == "css-display-none")
                );
            }
            assert!(
                native_bytecode(&fresh.json.replace("\"block\"", "\"ignore-previous-rules\""))
                    .is_none()
            );
        }
        assert!(native_bytecode("[]").is_none());
    }

    #[test]
    fn cosmetic_engine_omits_unused_network_rules_and_keeps_hide_exceptions() {
        use adblock::lists::{FilterSet, ParseOptions};
        let mut source: String = (0..200)
            .map(|index| format!("||unused-{index}.test^$image\n"))
            .collect();
        source.push_str(
            "##.adUnit\nsite.test##.sponsored\nsite.test#@#.adUnit\n\
             @@||exempt.test^$ghide\n@@||cancelled.test^$generichide\n\
             @@||cancelled.test^$generichide,badfilter\n",
        );
        let mut reference = FilterSet::new(false);
        reference.add_filter_list(source.clone(), ParseOptions::default());
        let reference = adblock::Engine::new_with_filter_set(reference);
        let mut focused = FilterSet::new(false);
        add_cosmetic_list(&mut focused, &source, ParseOptions::default());
        let focused = adblock::Engine::new_with_filter_set(focused);
        for address in [
            "https://site.test/",
            "https://exempt.test/",
            "https://cancelled.test/",
            "https://ordinary.test/",
        ] {
            let want = reference.url_cosmetic_resources(address);
            let got = focused.url_cosmetic_resources(address);
            assert_eq!(got.generichide, want.generichide, "{address}");
            assert_eq!(got.hide_selectors, want.hide_selectors, "{address}");
            assert_eq!(got.exceptions, want.exceptions, "{address}");
            assert_eq!(got.injected_script, want.injected_script, "{address}");
            assert_eq!(got.procedural_actions, want.procedural_actions, "{address}");
        }
        assert!(
            focused
                .url_cosmetic_resources("https://exempt.test/")
                .generichide
        );
        assert!(
            !focused
                .url_cosmetic_resources("https://cancelled.test/")
                .generichide
        );
        let request = adblock::request::Request::new(
            "https://unused-42.test/ad.png",
            "https://ordinary.test/",
            "image",
            "GET",
        )
        .expect("image request");
        assert!(reference.check_network_request(&request).should_block());
        assert!(!focused.check_network_request(&request).should_block());
        assert!(focused.serialize().len() < reference.serialize().len());
    }

    #[test]
    fn native_filter_json_is_stable_across_sets_and_preserves_exception_order() {
        use adblock::content_blocking::{CbAction, CbResourceType, CbRule, CbTrigger, CbType};
        let mut reference = None;
        for _ in 0..16 {
            let rule = CbRule {
                action: CbAction {
                    typ: CbType::Block,
                    selector: None,
                },
                trigger: CbTrigger {
                    url_filter: "ads".into(),
                    resource_type: Some(
                        [
                            CbResourceType::Document,
                            CbResourceType::Image,
                            CbResourceType::StyleSheet,
                            CbResourceType::Script,
                            CbResourceType::Font,
                            CbResourceType::Raw,
                            CbResourceType::SvgDocument,
                            CbResourceType::Media,
                            CbResourceType::Popup,
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..Default::default()
                },
            };
            let mut exception = rule.clone();
            exception.action.typ = CbType::IgnorePreviousRules;
            let json = native_rules_json(vec![rule, exception]).expect("serialize filters");
            let parsed: serde_json::Value = serde_json::from_str(&json).expect("filter JSON");
            assert_eq!(
                parsed[0]["trigger"]["resource-type"],
                serde_json::json!([
                    "document",
                    "font",
                    "image",
                    "media",
                    "popup",
                    "raw",
                    "script",
                    "style-sheet",
                    "svg-document"
                ])
            );
            assert_eq!(parsed[0]["action"]["type"], "block");
            assert_eq!(parsed[1]["action"]["type"], "ignore-previous-rules");
            if let Some(reference) = &reference {
                assert_eq!(&json, reference);
            } else {
                reference = Some(json);
            }
        }
    }
    #[test]
    fn refinement_privacy_separates_ad_and_tracker_requests() {
        let ad = "http://assets.example.org/ads/banners/check.png";
        let tracker = "http://assets.example.org/piwik.php";
        let page = "http://example.com/";
        assert!(!allows_image(ad, page, 2));
        assert!(!allows_image(tracker, page, 2));
        assert!(allows_image(ad, page, 6));
        assert!(!allows_image(tracker, page, 6));
        assert!(!allows_image(ad, page, 10));
        assert!(allows_image(tracker, page, 10));
    }
    #[test]
    fn cosmetic_cache_preserves_site_scope_generic_exceptions_and_new_classes() {
        let cnn = "https://edition.cnn.com/";
        assert!(
            cosmetic_rules(cnn, &[], &[])
                .iter()
                .any(|rule| rule.contains(".ad-slot-header__wrapper"))
        );
        assert!(
            !cosmetic_rules("https://example.org/", &[], &[])
                .iter()
                .any(|rule| rule.contains(".ad-slot-header__wrapper"))
        );
        let generic = vec!["ADBox".to_owned()];
        assert!(
            cosmetic_rules("https://example.org/", &generic, &[])
                .iter()
                .any(|rule| rule.contains(".ADBox"))
        );
        assert!(
            !cosmetic_rules("http://127.0.0.1/", &generic, &[])
                .iter()
                .any(|rule| rule.contains(".ADBox"))
        );
        for host in [
            "https://w3.org/",
            "https://mozilla.org/",
            "https://rust-lang.org/",
            "https://duckduckgo.com/",
        ] {
            cosmetic_rules(host, &[], &[]);
        }
        assert!(
            cosmetic_rules(cnn, &[], &[])
                .iter()
                .any(|rule| rule.contains(".ad-slot-header__wrapper"))
        );
        COSMETIC_URLS.with(|cache| assert_eq!(cache.borrow().len(), 4));
    }
}

#[cfg(test)]
mod youtube_tests {
    #[test]
    fn youtube_ad_rules_keep_skip_controls_available() {
        let css = super::cosmetic_rules(
            "https://www.youtube.com/watch?v=qualification",
            &["video-ads".into(), "ytp-ad-skip-button-container".into()],
            &[],
        );
        assert!(
            !css.iter().any(|rule| rule.starts_with(".video-ads {")
                || rule.contains(".ytp-ad-skip-button-container")),
            "{css:?}"
        );
    }
}

#[cfg(feature = "benchmark")]
pub(crate) fn benchmark() -> Result<serde_json::Value, String> {
    use sha2::Digest;
    let mut samples = Vec::new();
    for mode in [1, 2, 5, 6, 9, 10] {
        let compiled = compiled(mode)?;
        let mut value: serde_json::Value =
            serde_json::from_str(&compiled.json).map_err(|error| error.to_string())?;
        value.sort_all_objects();
        if let Some(rules) = value.as_array_mut() {
            for rule in rules {
                if let Some(types) = rule
                    .get_mut("trigger")
                    .and_then(|trigger| trigger.get_mut("resource-type"))
                    .and_then(serde_json::Value::as_array_mut)
                {
                    types.sort_unstable_by(|left, right| left.as_str().cmp(&right.as_str()));
                }
            }
        }
        let canonical = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
        let mut actions = std::collections::BTreeMap::<String, usize>::new();
        let network_rules = value
            .as_array()
            .ok_or("Expected filter array")?
            .iter()
            .filter(|rule| {
                let kind = rule["action"]["type"].as_str().unwrap_or("unknown");
                *actions.entry(kind.to_owned()).or_default() += 1;
                kind != "css-display-none"
            })
            .collect::<Vec<_>>();
        let network = serde_json::to_vec(&network_rules).map_err(|error| error.to_string())?;
        samples.push(serde_json::json!({
            "mode": mode,
            "rules": compiled.rules,
            "bytes": compiled.json.len(),
            "native_sha256": format!("{:x}", sha2::Sha256::digest(compiled.json.as_bytes())),
            "semantic_sha256": format!("{:x}", sha2::Sha256::digest(canonical)),
            "actions": actions,
            "network_sha256": format!("{:x}", sha2::Sha256::digest(network)),
        }));
    }
    Ok(serde_json::json!(samples))
}
