//! Compile bundled cosmetic rules with their scriptlet permission boundaries.

pub(crate) fn add_list(
    set: &mut adblock::lists::FilterSet,
    source: &str,
    options: adblock::lists::ParseOptions,
    full_network: bool,
) {
    use adblock::lists::{ParseOptions, ParsedLine, RuleTypes, parse_filter};
    if full_network {
        set.add_filter_list(source.to_owned(), options);
        return;
    }
    set.add_filter_list(
        source.to_owned(),
        ParseOptions {
            rule_types: RuleTypes::CosmeticOnly,
            ..options
        },
    );
    let network_options = ParseOptions {
        rule_types: RuleTypes::NetworkOnly,
        ..options
    };
    let mut generic_hide_exceptions = String::new();
    // Cosmetic matching consults network rules for generic-hide exceptions.
    for line in source.lines() {
        if let Ok(ParsedLine::Network(rule)) = parse_filter(line, false, network_options)
            && rule.is_generic_hide()
        {
            generic_hide_exceptions.push_str(line);
            generic_hide_exceptions.push('\n');
        }
    }
    if !generic_hide_exceptions.is_empty() {
        set.add_filter_list(generic_hide_exceptions, network_options);
    }
}

pub(crate) fn build(full_network: bool) -> adblock::Engine {
    let mut set = adblock::lists::FilterSet::new(false);
    let options = adblock::lists::ParseOptions {
        rule_types: adblock::lists::RuleTypes::All,
        ..Default::default()
    };
    for list in [
        include_str!("../resources/filters/easylist.txt"),
        include_str!("../resources/filters/easyprivacy.txt"),
    ] {
        add_list(&mut set, list, options, full_network);
    }
    for list in [
        include_str!("../resources/filters/ublock.txt"),
        include_str!("../resources/filters/ublock-quick-fixes.txt"),
    ] {
        add_list(
            &mut set,
            list,
            adblock::lists::ParseOptions {
                permissions: adblock::resources::PermissionMask::from_bits(1),
                ..options
            },
            full_network,
        );
    }
    add_list(
        &mut set,
        include_str!("../resources/filters/enzim-video.txt"),
        options,
        full_network,
    );
    adblock::Engine::new_with_filter_set(set)
}
