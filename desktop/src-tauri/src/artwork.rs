//! Match covers against a console's list of picture filenames.
//!
//! We use the picture as an application icon without asking, so a wrong
//! cover is worse than none. We try the following rules in order.
//!
//! 1. The scrubbed filename, exactly.
//! 2. The whole name, with punctuation collapsed, tags included.
//! 3. The title plus the country, ignoring every other bracketed tag.
//! 4. The bare title, and only a picture from a compatible country.
//! 5. Stop.
//!
//! We do no fuzzy matching, on purpose, and no title rewriting such as
//! moving "The" or folding roman numerals, which can turn "Mega Man X" into
//! "Mega Man 10".
//!
//! The country list is the No-Intro region vocabulary. To scrub filenames we
//! use the same character set as the RetroArch code that writes these files,
//! which is the set in [`scrub_filename`].

use std::collections::{HashMap, HashSet};

/// Characters we replace, as in RetroArch, in a stored box art filename.
/// `& * / : ` < > ? \ | "` become `_`.
pub fn scrub_filename(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            '&' | '*' | '/' | ':' | '`' | '<' | '>' | '?' | '\\' | '|' | '"' => '_',
            other => other,
        })
        .collect()
}

/// Where we download a matched picture from.
///
/// `raw.githubusercontent.com` returns the text of a pointer file (about four
/// percent of covers are stored that way) with a success status. This host
/// returns the picture under the pointer. We keep the spaces in the catalogue
/// name, because the thumbnail repositories use that form.
pub fn artwork_download_url(catalog: &str, filename: &str) -> String {
    let catalog = percent_encode(catalog);
    let filename = percent_encode(filename);
    format!("https://thumbnails.libretro.com/{catalog}/Named_Boxarts/{filename}.png")
}

/// The git tree that lists one console's picture filenames.
/// Picture filenames from one GitHub git-tree response.
///
/// We refuse a truncated tree, because if we matched against half the list we
/// would report "no cover" for pictures that exist.
pub fn filenames_from_git_tree(bytes: &[u8]) -> Result<Vec<String>, &'static str> {
    let parsed: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "the picture list could not be read")?;
    if parsed
        .get("truncated")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
    {
        return Err("the picture list was incomplete");
    }
    let names: Vec<String> = parsed
        .get("tree")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.get("path")?.as_str()?;
            let rest = path.strip_prefix("Named_Boxarts/")?;
            rest.strip_suffix(".png")
                .or_else(|| rest.strip_suffix(".PNG"))
                .map(str::to_owned)
        })
        .collect();
    if names.is_empty() {
        return Err("the picture list was empty");
    }
    Ok(names)
}

/// Whether the two names are the same game. We use this to show that we did
/// not match a name to another game's cover.
pub fn same_game_title(catalog_name: &str, artwork_filename: &str) -> bool {
    let wanted = NameParts::parse(&scrub_filename(catalog_name));
    let found = NameParts::parse(artwork_filename);
    !wanted.title.is_empty() && wanted.title == found.title
}

pub fn artwork_index_url(catalog: &str) -> String {
    let repository = percent_encode(&catalog.replace(' ', "_"));
    format!(
        "https://api.github.com/repos/libretro-thumbnails/{repository}/git/trees/master?recursive=1"
    )
}

fn percent_encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// The comparison by which we found a cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRung {
    Exact,
    WholeName,
    TitleAndCountry,
    TitleInCompatibleCountry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverMatch {
    pub filename: String,
    pub rung: MatchRung,
}

/// One console's picture filenames, indexed for the rules above.
#[derive(Debug, Clone)]
pub struct ArtworkIndex {
    by_filename: HashMap<String, String>,
    by_whole_name: HashMap<String, Vec<String>>,
    by_title_country: HashMap<(String, String), Vec<String>>,
    by_title: HashMap<String, Vec<String>>,
    meta: HashMap<String, NameParts>,
}

impl ArtworkIndex {
    pub fn from_filenames<I, S>(filenames: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut index = Self {
            by_filename: HashMap::new(),
            by_whole_name: HashMap::new(),
            by_title_country: HashMap::new(),
            by_title: HashMap::new(),
            meta: HashMap::new(),
        };
        for filename in filenames {
            index.insert(filename.as_ref());
        }
        index
    }

    pub fn len(&self) -> usize {
        self.by_filename.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_filename.is_empty()
    }

    fn insert(&mut self, filename: &str) {
        let parts = NameParts::parse(filename);
        self.by_filename
            .insert(filename.to_owned(), filename.to_owned());
        self.by_whole_name
            .entry(parts.whole_name.clone())
            .or_default()
            .push(filename.to_owned());
        self.by_title_country
            .entry((parts.title.clone(), parts.country_key.clone()))
            .or_default()
            .push(filename.to_owned());
        self.by_title
            .entry(parts.title.clone())
            .or_default()
            .push(filename.to_owned());
        self.meta.insert(filename.to_owned(), parts);
    }
}

/// The cover for a catalogue name, or nothing.
///
/// We never match an empty title with the later rules, because then we would
/// give one picture to every name we failed to parse.
pub fn match_cover(index: &ArtworkIndex, catalog_name: &str) -> Option<CoverMatch> {
    let scrubbed = scrub_filename(catalog_name);
    if let Some(filename) = index.by_filename.get(&scrubbed) {
        return Some(CoverMatch {
            filename: filename.clone(),
            rung: MatchRung::Exact,
        });
    }

    let wanted = NameParts::parse(&scrubbed);
    if wanted.title.is_empty() {
        return None;
    }

    if let Some(filename) = pick(index, index.by_whole_name.get(&wanted.whole_name), &wanted) {
        return Some(CoverMatch {
            filename,
            rung: MatchRung::WholeName,
        });
    }
    let key = (wanted.title.clone(), wanted.country_key.clone());
    if let Some(filename) = pick(index, index.by_title_country.get(&key), &wanted) {
        return Some(CoverMatch {
            filename,
            rung: MatchRung::TitleAndCountry,
        });
    }
    let compatible = index.by_title.get(&wanted.title).map(|candidates| {
        candidates
            .iter()
            .filter(|filename| {
                countries_compatible(
                    &wanted.countries,
                    &index.meta.get(*filename).expect("indexed").countries,
                )
            })
            .cloned()
            .collect::<Vec<_>>()
    });
    pick(index, compatible.as_ref(), &wanted).map(|filename| CoverMatch {
        filename,
        rung: MatchRung::TitleInCompatibleCountry,
    })
}

/// Whether any picture in the index has this same title, whatever the country.
/// This is the upper limit, because beyond it we would match another game's cover.
pub fn title_has_any_picture(index: &ArtworkIndex, catalog_name: &str) -> bool {
    let wanted = NameParts::parse(&scrub_filename(catalog_name));
    !wanted.title.is_empty() && index.by_title.contains_key(&wanted.title)
}

fn pick(
    index: &ArtworkIndex,
    candidates: Option<&Vec<String>>,
    wanted: &NameParts,
) -> Option<String> {
    let candidates = candidates?;
    candidates
        .iter()
        .min_by(|left, right| rank(index, left, wanted).cmp(&rank(index, right, wanted)))
        .cloned()
}

fn rank(index: &ArtworkIndex, filename: &str, wanted: &NameParts) -> (u8, u8, usize, String) {
    let found = index.meta.get(filename).expect("indexed picture");
    (
        country_rank(&wanted.countries, &found.countries),
        cosmetic_penalty(wanted, found),
        filename.chars().count(),
        filename.to_owned(),
    )
}

/// Retail releases. We also exclude a Mega Drive Mini tag with `Mini`, on
/// purpose, so we always count the cover rate over the same set of releases.
pub fn is_retail_name(name: &str) -> bool {
    const MARKERS: &[&str] = &[
        "Beta",
        "Proto",
        "Prototype",
        "Demo",
        "Sample",
        "Unl",
        "Aftermarket",
        "Pirate",
        "Program",
        "Test",
        "Debug",
        "Virtual Console",
        "Classic Collection",
        "Mega Collection",
        "Mini",
    ];
    !MARKERS.iter().any(|marker| name.contains(marker))
}

#[derive(Debug, Clone)]
struct NameParts {
    title: String,
    whole_name: String,
    countries: HashSet<String>,
    country_key: String,
    cosmetics: HashMap<&'static str, HashSet<String>>,
}

impl NameParts {
    fn parse(name: &str) -> Self {
        let (title, groups) = split_trailing_tags(name);
        let title_key = normalize_token(title);
        let mut countries = HashSet::new();
        let mut cosmetics: HashMap<&'static str, HashSet<String>> = HashMap::new();
        let mut saw_country = false;
        let mut whole_groups = Vec::new();
        for group in &groups {
            whole_groups.push(normalize_token(group));
            let class = classify_tag(group);
            if class == "region" && !saw_country {
                saw_country = true;
                for part in group.split(',') {
                    let part = part.trim().to_ascii_lowercase();
                    if !part.is_empty() {
                        countries.insert(part);
                    }
                }
                continue;
            }
            if class == "region" {
                continue;
            }
            cosmetics
                .entry(class)
                .or_default()
                .insert(group.trim().to_ascii_lowercase());
        }
        whole_groups.sort();
        let mut country_list: Vec<_> = countries.iter().cloned().collect();
        country_list.sort();
        Self {
            title: title_key.clone(),
            whole_name: format!("{title_key}||{}", whole_groups.join("|")),
            countries,
            country_key: country_list.join(","),
            cosmetics,
        }
    }
}

fn cosmetic_penalty(wanted: &NameParts, found: &NameParts) -> u8 {
    ["dev", "lic", "version", "lang", "other"]
        .into_iter()
        .filter(|class| wanted.cosmetics.get(class) != found.cosmetics.get(class))
        .count() as u8
}

fn countries_compatible(wanted: &HashSet<String>, found: &HashSet<String>) -> bool {
    if wanted.is_empty() || found.is_empty() {
        return true;
    }
    if wanted.iter().any(|country| found.contains(country)) {
        return true;
    }
    wanted.contains("world") || found.contains("world")
}

fn country_rank(wanted: &HashSet<String>, found: &HashSet<String>) -> u8 {
    if !wanted.is_empty() && wanted == found {
        return 0;
    }
    if !wanted.is_empty() && wanted.iter().any(|country| found.contains(country)) {
        return 1;
    }
    if found.contains("world") {
        return 2;
    }
    const PREFERENCE: &[&str] = &[
        "world",
        "usa",
        "europe",
        "japan",
        "asia",
        "australia",
        "korea",
        "brazil",
        "taiwan",
        "china",
        "canada",
        "france",
        "germany",
        "spain",
        "italy",
        "netherlands",
        "sweden",
        "unknown",
    ];
    PREFERENCE
        .iter()
        .position(|country| found.contains(*country))
        .map(|position| 3 + position as u8)
        .unwrap_or(99)
}

/// Peel every trailing `(...)` and `[...]` group. A parenthesis that is part
/// of the title, with more words after it, stays in the title.
fn split_trailing_tags(name: &str) -> (&str, Vec<&str>) {
    let mut end = name.trim_end().len();
    let mut groups = Vec::new();
    loop {
        let rest = name[..end].trim_end();
        end = rest.len();
        let close = if rest.ends_with(')') {
            ')'
        } else if rest.ends_with(']') {
            ']'
        } else {
            break;
        };
        let open = if close == ')' { '(' } else { '[' };
        let Some(start) = rest.rfind(open) else {
            break;
        };
        let inner = &rest[start + open.len_utf8()..end - close.len_utf8()];
        if inner.contains(open) || inner.contains(close) {
            break;
        }
        groups.push(inner);
        end = start;
        if end == 0 {
            break;
        }
    }
    groups.reverse();
    (name[..end].trim_end(), groups)
}

fn classify_tag(group: &str) -> &'static str {
    let parts: Vec<String> = group
        .split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect();
    if !parts.is_empty() && parts.iter().all(|part| REGIONS.contains(&part.as_str())) {
        return "region";
    }
    if is_language(group) {
        return "lang";
    }
    if is_version(group) {
        return "version";
    }
    if is_development(group) {
        return "dev";
    }
    if is_licence(group) {
        return "lic";
    }
    "other"
}

fn is_language(group: &str) -> bool {
    let compact: String = group
        .chars()
        .filter(|character| *character != ' ')
        .collect();
    if compact.is_empty() {
        return false;
    }
    compact.split(['+', ',']).all(|part| {
        let mut chars = part.chars();
        matches!(
            (chars.next(), chars.next(), chars.next()),
            (Some(first), Some(second), None)
                if first.is_ascii_uppercase() && second.is_ascii_lowercase()
        )
    })
}

fn is_version(group: &str) -> bool {
    let lower = group.trim().to_ascii_lowercase();
    if lower.starts_with("version ") || lower == "alt" || lower.starts_with("alt ") {
        return lower == "alt"
            || lower.starts_with("version ")
            || lower[4..].trim().chars().all(|c| c.is_ascii_digit());
    }
    if let Some(rest) = lower.strip_prefix('v') {
        return rest.starts_with(|character: char| character.is_ascii_digit() || character == '.');
    }
    if let Some(rest) = lower.strip_prefix("rev") {
        let rest = rest.trim_start();
        return !rest.is_empty() && !rest.contains(' ');
    }
    false
}

fn is_development(group: &str) -> bool {
    let lower = group.trim().to_ascii_lowercase();
    [
        "beta",
        "proto",
        "prototype",
        "sample",
        "demo",
        "preview",
        "kiosk",
        "debug",
        "test",
    ]
    .iter()
    .any(|prefix| lower == *prefix || lower.starts_with(&format!("{prefix} ")))
        || lower.starts_with("prototype ")
}

fn is_licence(group: &str) -> bool {
    let lower = group.trim().to_ascii_lowercase();
    [
        "unl",
        "unlicensed",
        "aftermarket",
        "pirate",
        "homebrew",
        "licensed",
    ]
    .iter()
    .any(|prefix| lower == *prefix || lower.starts_with(&format!("{prefix} ")))
}

/// Lowercase, and treat punctuation as a space. Roman numerals stay letters,
/// a leading "The" stays where it is, and we leave accents alone.
fn normalize_token(value: &str) -> String {
    let mut words = Vec::new();
    let mut word = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            word.push(character.to_ascii_lowercase());
            continue;
        }
        if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words.join(" ")
}

const REGIONS: &[&str] = &[
    "usa",
    "europe",
    "japan",
    "world",
    "asia",
    "australia",
    "brazil",
    "canada",
    "china",
    "korea",
    "taiwan",
    "france",
    "germany",
    "spain",
    "italy",
    "netherlands",
    "sweden",
    "norway",
    "denmark",
    "finland",
    "greece",
    "poland",
    "portugal",
    "russia",
    "uk",
    "united kingdom",
    "hong kong",
    "israel",
    "mexico",
    "argentina",
    "belgium",
    "austria",
    "switzerland",
    "india",
    "south africa",
    "ireland",
    "new zealand",
    "turkey",
    "latin america",
    "scandinavia",
    "unknown",
    "middle east",
    "croatia",
    "czech",
    "hungary",
    "slovakia",
    "ukraine",
    "south korea",
    "uae",
    "thailand",
    "singapore",
    "indonesia",
    "malaysia",
    "philippines",
    "vietnam",
    "puerto rico",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn index(names: &[&str]) -> ArtworkIndex {
        ArtworkIndex::from_filenames(names.iter().copied())
    }

    #[test]
    fn an_exact_filename_wins_before_any_looser_rule() {
        let covers = index(&["Tiny Adventure (USA)", "Tiny Adventure (USA) (Rev 1)"]);
        let matched = match_cover(&covers, "Tiny Adventure (USA)").unwrap();
        assert_eq!(matched.filename, "Tiny Adventure (USA)");
        assert_eq!(matched.rung, MatchRung::Exact);
    }

    #[test]
    fn cosmetic_tags_are_ignored_and_the_country_is_kept() {
        let covers = index(&["Bible Adventures (USA) (Unl)"]);
        let matched = match_cover(&covers, "Bible Adventures (USA)").unwrap();
        assert_eq!(matched.filename, "Bible Adventures (USA) (Unl)");
        assert_eq!(matched.rung, MatchRung::TitleAndCountry);

        let lock_on = index(&["Sonic & Knuckles + Sonic the Hedgehog 3 (USA)"]);
        let matched = match_cover(
            &lock_on,
            "Sonic & Knuckles + Sonic the Hedgehog 3 (USA) (Lock-On)",
        )
        .unwrap();
        assert_eq!(
            matched.filename,
            "Sonic & Knuckles + Sonic the Hedgehog 3 (USA)"
        );
    }

    #[test]
    fn the_bare_title_may_use_a_world_picture_and_not_another_country() {
        let covers = index(&["Sonic the Hedgehog (Japan)", "Sonic the Hedgehog (World)"]);
        let matched = match_cover(&covers, "Sonic the Hedgehog (USA) (Rev A)").unwrap();
        assert_eq!(matched.filename, "Sonic the Hedgehog (World)");
        assert_eq!(matched.rung, MatchRung::TitleInCompatibleCountry);

        let japan_only = index(&["Sonic the Hedgehog (Japan)"]);
        assert!(match_cover(&japan_only, "Sonic the Hedgehog (USA)").is_none());
    }

    #[test]
    fn a_shared_country_in_a_combined_tag_is_compatible() {
        let covers = index(&["Asteroids (USA, Europe)"]);
        let matched = match_cover(&covers, "Asteroids (USA)").unwrap();
        assert_eq!(matched.filename, "Asteroids (USA, Europe)");
    }

    #[test]
    fn a_different_game_is_never_given_a_nearby_cover() {
        let covers = index(&[
            "Alien 3 (USA, Europe)",
            "Streets of Rage (World)",
            "Mega Man 10 (USA)",
        ]);
        assert!(match_cover(&covers, "Alien Games (USA) (Proto)").is_none());
        assert!(match_cover(&covers, "Street Hero (USA) (Proto 1)").is_none());
        assert!(match_cover(&covers, "Mega Man X (USA)").is_none());
        assert_ne!(
            normalize_token("Mega Man X"),
            normalize_token("Mega Man 10")
        );
    }

    #[test]
    fn a_parenthesis_inside_the_title_is_not_cut_off_early() {
        let covers = index(&["Diguo Wangchao (Ya Se Chuanshuo) (Taiwan)"]);
        let matched =
            match_cover(&covers, "Diguo Wangchao (Ya Se Chuanshuo) (Taiwan) (Rev 1)").unwrap();
        assert_eq!(
            matched.filename,
            "Diguo Wangchao (Ya Se Chuanshuo) (Taiwan)"
        );
    }

    #[test]
    fn scrubbed_characters_still_match_the_stored_filename() {
        let covers = index(&["Tom _ Jerry (USA)"]);
        let matched = match_cover(&covers, "Tom & Jerry (USA)").unwrap();
        assert_eq!(matched.rung, MatchRung::Exact);
        assert!(artwork_download_url("Atari - 5200", "Astro Chase (USA)")
            .starts_with("https://thumbnails.libretro.com/"));
        assert!(!artwork_download_url("Atari - 5200", "Astro Chase (USA)")
            .contains("raw.githubusercontent.com"));
    }

    #[test]
    fn the_closer_cosmetic_tag_wins_the_tie() {
        let covers = index(&[
            "Happy Camper (USA) (Proto) (Unl)",
            "Happy Camper (USA) (Proto)",
        ]);
        let matched = match_cover(&covers, "Happy Camper (USA) (Proto)").unwrap();
        assert_eq!(matched.filename, "Happy Camper (USA) (Proto)");
    }
}
