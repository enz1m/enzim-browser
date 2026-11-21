//! Spelling marks in what a person types into pages. The engine checks
//! words with the desktop's spelling service (enchant) and its dictionaries;
//! the browser chooses the languages and offers the corrections in its own
//! menu.

use super::*;
use enzui_webkit::page::SpellingFix;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) enum SpellingCommand {
    /// Check spelling, or do not.
    Enabled(bool),
    /// Check in this language too, or no longer.
    Language(String, bool),
    /// Correct, accept or learn the word the menu was opened on.
    Fix(SpellingFix),
}

/// Where the spelling service looks for hunspell dictionaries: the person's
/// own folder (which `ENCHANT_CONFIG_DIR` moves), then the system's.
fn dictionary_folders() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let own = std::env::var_os("ENCHANT_CONFIG_DIR").map_or_else(
        || {
            std::env::var_os("XDG_CONFIG_HOME")
                .map_or_else(|| home.join(".config"), PathBuf::from)
                .join("enchant")
        },
        PathBuf::from,
    );
    let mut folders = vec![own.join("hunspell")];
    for system in [
        "/usr/share/hunspell",
        "/usr/share/myspell",
        "/usr/share/myspell/dicts",
        "/usr/local/share/hunspell",
    ] {
        folders.push(PathBuf::from(system));
    }
    folders
}

/// The languages that have a dictionary in `folders`: a word list
/// (`en_US.dic`) beside its rules (`en_US.aff`), named as a locale.
pub(crate) fn dictionaries_in(folders: &[PathBuf]) -> Vec<String> {
    let mut found = std::collections::BTreeSet::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten().take(4096) {
            let path = entry.path();
            let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if path.extension().is_some_and(|ext| ext == "dic")
                && locale_name(name)
                && path.with_extension("aff").is_file()
            {
                found.insert(name.to_owned());
            }
        }
    }
    found.into_iter().collect()
}

/// `en`, `en_US`, `sr_RS@latin`: what a dictionary for a language is named,
/// a language and where given its country. Hyphenation and thesaurus files
/// (`hyph_en_US`, `th_en_US_v2`) are not.
fn locale_name(name: &str) -> bool {
    let base = name.split('@').next().unwrap_or_default();
    let mut parts = base.split('_');
    let language = parts.next().unwrap_or_default();
    let country = parts.next().is_none_or(|country| {
        (2..=3).contains(&country.len())
            && country
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    });
    (2..=3).contains(&language.len())
        && language.chars().all(|c| c.is_ascii_lowercase())
        && country
        && name.len() <= 24
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '@'))
}

/// The locale a person's desktop is in, as dictionaries are named: `en_US`
/// from `en_US.UTF-8`. `None` for the C locale.
pub(crate) fn desktop_locale(locale: Option<&str>) -> Option<String> {
    let name = locale?.split(['.', '@']).next()?;
    (locale_name(name) && !name.eq_ignore_ascii_case("c")).then(|| name.to_owned())
}

/// The languages spelling is checked in. What the person chose, where they
/// chose; otherwise the desktop's language, and where that has no dictionary
/// of its own, one of the same language from another country. Empty where
/// checking is off.
pub(crate) fn languages(
    enabled: bool,
    chosen: &[String],
    locale: Option<&str>,
    found: &[String],
) -> Vec<String> {
    if !enabled {
        return Vec::new();
    }
    if !chosen.is_empty() {
        return chosen.to_vec();
    }
    let Some(locale) = locale else {
        // No language to go by: the first dictionary there is.
        return found.first().cloned().into_iter().collect();
    };
    if found.is_empty() || found.iter().any(|name| name == locale) {
        // With no dictionary found here the service may still know one
        // (it has other providers than hunspell): ask for the language.
        return vec![locale.to_owned()];
    }
    let language = locale.split('_').next().unwrap_or(locale);
    found
        .iter()
        .find(|name| name.split('_').next() == Some(language))
        .or_else(|| found.first())
        .cloned()
        .into_iter()
        .collect()
}

fn locale_from_environment() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .and_then(|value| desktop_locale(Some(&value)))
}

/// The dictionaries on this machine, looked for once: installing one shows
/// at the next start.
fn found_dictionaries() -> &'static [String] {
    static FOUND: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| dictionaries_in(&dictionary_folders()))
}

/// The menu entries for a misspelled word: what to put in its place, then
/// learning and accepting it. Learning writes the word to the person's word
/// list for good, which a private window (`learn` false) does not do.
pub(crate) fn menu_entries(guesses: &[String], learn: bool) -> Vec<(String, Option<SpellingFix>)> {
    let mut entries: Vec<(String, Option<SpellingFix>)> = guesses
        .iter()
        .enumerate()
        .map(|(index, word)| (word.clone(), Some(SpellingFix::Guess(index))))
        .collect();
    if entries.is_empty() {
        entries.push(("No spelling suggestions".to_owned(), None));
    }
    if learn {
        entries.push(("Add to dictionary".to_owned(), Some(SpellingFix::Learn)));
    }
    entries.push(("Ignore spelling".to_owned(), Some(SpellingFix::Ignore)));
    entries
}

impl App {
    /// The languages spelling is checked in now.
    pub(crate) fn spelling_languages(&self) -> Vec<String> {
        languages(
            self.browser.saved.spell_check,
            &self.browser.saved.spell_languages,
            locale_from_environment().as_deref(),
            found_dictionaries(),
        )
    }

    /// Tells the engine: one setting for every page.
    fn apply_spelling(&mut self) {
        let languages = self.spelling_languages();
        self.submit_browser_action(BrowserAction::SpellCheck(languages));
        if !cfg!(test) {
            self.browser.save();
        }
    }

    pub(super) fn spelling_command(&mut self, command: SpellingCommand) -> Task<Message> {
        match command {
            SpellingCommand::Enabled(enabled) => {
                self.browser.saved.spell_check = enabled;
                self.apply_spelling();
            }
            SpellingCommand::Language(language, on) => {
                if self.browser.saved.spell_languages.is_empty() {
                    // The first choice starts from what is checked now.
                    self.browser.saved.spell_languages = self.spelling_languages();
                }
                let chosen = &mut self.browser.saved.spell_languages;
                chosen.retain(|name| *name != language);
                if on && chosen.len() < 8 {
                    chosen.push(language);
                }
                self.apply_spelling();
            }
            SpellingCommand::Fix(fix) => {
                self.submit_browser_action(BrowserAction::Spelling(fix));
            }
        }
        Task::none()
    }

    pub(super) fn spelling_settings(&self) -> Element<'_, Message> {
        use iced::widget::{column, row};
        let found = found_dictionaries();
        let checked = self.spelling_languages();
        let subtitle = if found.is_empty() {
            "No dictionary was found on this computer. Install hunspell and a dictionary for your language (the package is often named hunspell-en_us), then start the browser again.".to_owned()
        } else {
            format!(
                "Underlines words a dictionary does not know, in text you type. Right-click one for corrections. Checking: {}.",
                if checked.is_empty() {
                    "nothing".to_owned()
                } else {
                    checked.join(", ")
                }
            )
        };
        let switch = enzui_widgets::preferences_row::enz_preferences_row("Check spelling")
            .icon("spell-check")
            .subtitle(subtitle)
            .switch(self.browser.saved.spell_check, |value| {
                c(Command::Spelling(SpellingCommand::Enabled(value)))
            })
            .into_element();
        if found.len() < 2 || !self.browser.saved.spell_check {
            return switch;
        }
        let mut languages = row![].spacing(6);
        for name in found.iter().take(12) {
            let on = checked.contains(name);
            languages = languages.push(
                enzui_widgets::button::enz_button(
                    name.clone(),
                    c(Command::Spelling(SpellingCommand::Language(name.clone(), !on))),
                )
                .kind(if on {
                    enzui_widgets::button::ButtonKind::Accent
                } else {
                    enzui_widgets::button::ButtonKind::Quiet
                })
                .size(enzui_widgets::button::ButtonSize::Sm)
                .into_element(),
            );
        }
        column![
            switch,
            iced::widget::container(iced::widget::scrollable(languages).direction(
                iced::widget::scrollable::Direction::Horizontal(
                    iced::widget::scrollable::Scrollbar::new()
                )
            ))
            .padding([0, 12])
        ]
        .spacing(4)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_dictionary_is_a_word_list_beside_its_rules_named_as_a_language() {
        let dir = std::env::temp_dir().join(format!("enzim-spell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        for file in [
            "en_US.dic",
            "en_US.aff",
            "hu_HU.dic",
            "hu_HU.aff",
            // Rules are missing.
            "de_DE.dic",
            // Not word lists for checking.
            "hyph_en_US.dic",
            "hyph_en_US.aff",
            "th_en_US_v2.dic",
            "th_en_US_v2.aff",
        ] {
            std::fs::write(dir.join(file), b"").expect("file");
        }
        let missing = dir.join("none");
        assert_eq!(
            dictionaries_in(&[missing, dir.clone(), dir.clone()]),
            ["en_US", "hu_HU"]
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(locale_name("sr_RS@latin"));
        assert!(!locale_name("../en_US"));
    }

    #[test]
    fn spelling_is_checked_in_the_desktops_language_unless_the_person_chose() {
        let found = names(&["de_DE", "en_GB", "hu_HU"]);
        assert_eq!(desktop_locale(Some("hu_HU.UTF-8")).as_deref(), Some("hu_HU"));
        assert_eq!(desktop_locale(Some("C")), None);
        assert_eq!(desktop_locale(Some("C.UTF-8")), None);
        assert_eq!(languages(true, &[], Some("hu_HU"), &found), ["hu_HU"]);
        // No dictionary for the country: the language's from another one.
        assert_eq!(languages(true, &[], Some("en_US"), &found), ["en_GB"]);
        // None for the language at all: the first there is.
        assert_eq!(languages(true, &[], Some("fr_FR"), &found), ["de_DE"]);
        // Nothing found here: the service is asked for the language anyway.
        assert_eq!(languages(true, &[], Some("fr_FR"), &[]), ["fr_FR"]);
        assert_eq!(languages(true, &[], None, &found), ["de_DE"]);
        assert!(languages(true, &[], None, &[]).is_empty());
        // The person's choice stands, and off is off.
        let chosen = names(&["de_DE", "hu_HU"]);
        assert_eq!(languages(true, &chosen, Some("en_US"), &found), chosen);
        assert!(languages(false, &chosen, Some("en_US"), &found).is_empty());
    }

    #[test]
    fn a_misspelled_word_is_offered_its_corrections_then_learning_and_ignoring() {
        let entries = menu_entries(&names(&["hello", "hallo"]), true);
        assert_eq!(
            entries,
            [
                ("hello".to_owned(), Some(SpellingFix::Guess(0))),
                ("hallo".to_owned(), Some(SpellingFix::Guess(1))),
                ("Add to dictionary".to_owned(), Some(SpellingFix::Learn)),
                ("Ignore spelling".to_owned(), Some(SpellingFix::Ignore)),
            ]
        );
        // A word nothing is known for can still be learned.
        let entries = menu_entries(&[], true);
        assert_eq!(entries[0], ("No spelling suggestions".to_owned(), None));
        assert_eq!(entries.len(), 3);
        // A private window leaves no word behind.
        let entries = menu_entries(&names(&["hello"]), false);
        assert!(entries.iter().all(|(_, fix)| *fix != Some(SpellingFix::Learn)));
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn the_person_turns_checking_off_and_chooses_languages() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        assert!(app.browser.saved.spell_check);
        let command = |command| c(Command::Spelling(command));
        let _ = app.update(command(SpellingCommand::Language("hu_HU".into(), true)));
        assert!(app.browser.saved.spell_languages.contains(&"hu_HU".to_owned()));
        assert!(app.spelling_languages().contains(&"hu_HU".to_owned()));
        let _ = app.update(command(SpellingCommand::Language("hu_HU".into(), false)));
        assert!(!app.browser.saved.spell_languages.contains(&"hu_HU".to_owned()));
        let _ = app.update(command(SpellingCommand::Enabled(false)));
        assert!(app.spelling_languages().is_empty());
    }
}
