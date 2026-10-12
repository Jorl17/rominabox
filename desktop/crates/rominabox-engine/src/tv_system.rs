//! Whether a game is from a PAL country, from its name in the No-Intro
//! catalog. We use it to recommend the PAL picture filters of a console.

/// The countries in No-Intro region tags whose televisions were PAL, in the
/// spelling of the tags. We leave out Brazil, whose PAL-M ran at 60 Hz as
/// NTSC did, and the tags of several regions, such as Asia.
const PAL_COUNTRIES: &[&str] = &[
    "europe",
    "australia",
    "new zealand",
    "uk",
    "united kingdom",
    "ireland",
    "france",
    "germany",
    "spain",
    "italy",
    "portugal",
    "netherlands",
    "belgium",
    "austria",
    "switzerland",
    "sweden",
    "norway",
    "denmark",
    "finland",
    "scandinavia",
    "greece",
    "poland",
    "russia",
    "croatia",
    "czech",
    "hungary",
    "slovakia",
    "ukraine",
    "turkey",
    "south africa",
];

/// Whether the game named `name` in the No-Intro catalog is only for PAL
/// countries: "Sonic (Europe)" is, and "Sonic (USA, Europe)" and
/// "Sonic (World)" are not.
pub fn pal_only(name: &str) -> bool {
    let countries = crate::artwork::countries(name);
    !countries.is_empty() && countries.iter().all(|country| PAL_COUNTRIES.contains(&country.as_str()))
}

#[cfg(test)]
mod tests {
    use super::pal_only;

    #[test]
    fn a_game_is_pal_only_when_every_country_in_its_region_is_pal() {
        for name in ["Sonic the Hedgehog (Europe)", "Asterix (Germany)", "Lemmings (France, Spain)", "Game (Australia)"] {
            assert!(pal_only(name), "{name}");
        }
        for name in [
            "Sonic the Hedgehog (USA, Europe)",
            "Sonic the Hedgehog (World)",
            "Sonic the Hedgehog (Japan)",
            "Game (Brazil)",
            "Sonic the Hedgehog",
            "Sonic the Hedgehog (Rev 1)",
        ] {
            assert!(!pal_only(name), "{name}");
        }
    }
}
