pub(crate) struct RuleBundle {
    pub(crate) json: String,
    pub(crate) input: usize,
    pub(crate) converted: usize,
    pub(crate) rules: usize,
}

pub(crate) fn ads_enabled(mode: usize) -> bool {
    mode > 0 && mode & 4 == 0
}
pub(crate) fn sources(mode: usize) -> Vec<&'static str> {
    let mut sources = Vec::new();
    if ads_enabled(mode) {
        sources.push(include_str!("../resources/filters/easylist.txt"));
    }
    if mode > 0 && mode & 8 == 0 {
        sources.push(include_str!("../resources/filters/easyprivacy.txt"));
    }
    sources
}
pub(crate) fn native_rules_json(
    rules: Vec<adblock::content_blocking::CbRule>,
) -> Result<String, String> {
    let mut bytes = Vec::new();
    bytes.push(b'[');
    for (index, rule) in rules.into_iter().enumerate() {
        if index != 0 {
            bytes.push(b',');
        }
        let mut value = serde_json::to_value(rule).map_err(|error| error.to_string())?;
        if let Some(types) = value
            .get_mut("trigger")
            .and_then(|trigger| trigger.get_mut("resource-type"))
            .and_then(serde_json::Value::as_array_mut)
        {
            // Resource types are a set; stable order permits native cache reuse.
            types.sort_unstable_by(|left, right| left.as_str().cmp(&right.as_str()));
        }
        serde_json::to_writer(&mut bytes, &value).map_err(|error| error.to_string())?;
    }
    bytes.push(b']');
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

pub(crate) fn convert(mode: usize, native_cosmetics: bool) -> Result<RuleBundle, String> {
    let sources = sources(mode);
    let input = sources
        .iter()
        .flat_map(|s| s.lines())
        .filter(|s| !s.trim().is_empty() && !s.starts_with('!') && !s.starts_with('['))
        .count();
    let mut set = adblock::lists::FilterSet::new(true);
    for source in sources {
        set.add_filter_list(source.to_owned(), adblock::lists::ParseOptions::default());
    }
    let (mut rules, _used) = set
        .into_content_blocking()
        .map_err(|()| "Filter list conversion failed.".to_owned())?;
    let converted = _used.len();
    drop(_used);
    if mode & 3 != 2 {
        use adblock::content_blocking::{CbLoadType, CbType};
        rules.retain_mut(|rule| {
            if rule.action.typ == CbType::Block {
                if !rule.trigger.load_type.is_empty()
                    && rule
                        .trigger
                        .load_type
                        .iter()
                        .all(|kind| *kind == CbLoadType::FirstParty)
                {
                    return false;
                }
                rule.trigger.load_type = vec![CbLoadType::ThirdParty];
            }
            true
        });
    }
    if ads_enabled(mode) {
        let mut player_ads = adblock::lists::FilterSet::new(true);
        player_ads.add_filter_list(
            include_str!("../resources/filters/enzim-video-network.txt").into(),
            adblock::lists::ParseOptions::default(),
        );
        let (player_rules, _) = player_ads
            .into_content_blocking()
            .map_err(|()| "Video ad filter conversion failed")?;
        rules.extend(player_rules);
    }
    if !native_cosmetics {
        rules.retain(|rule| rule.action.typ != adblock::content_blocking::CbType::CssDisplayNone);
    }
    let result = RuleBundle {
        rules: rules.len(),
        converted,
        input,
        json: native_rules_json(rules)?,
    };
    Ok(result)
}
