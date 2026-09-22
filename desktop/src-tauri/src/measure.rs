//! The cover-rate measurement. We ignore it in the ordinary tests and run it
//! in the identification tests. We download each catalogue and picture list
//! once into work/identification-cache and then match offline.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use crate::artwork::{self, MatchRung};
use crate::systems;

#[test]
#[ignore]
fn cover_rates_against_the_published_picture_lists() {
    let cache = crate::repo::at("work/identification-cache");
    fs::create_dir_all(cache.join("dats")).unwrap();
    fs::create_dir_all(cache.join("trees")).unwrap();

    let mut cartridge = Totals::default();
    let mut disc = Totals::default();
    let mut misses = Vec::new();
    let mut left: Vec<String> = Vec::new();
    println!(
        "{:<20} {:>7} {:>7} {:>8} {:>8} {:>8}",
        "console", "names", "retail", "before", "after", "ceiling"
    );
    for system in systems::registry() {
        let Some(catalog) = system.catalog.as_deref() else {
            continue;
        };
        let disc_catalog = system.category == "disc";
        let names = distinct_names(&read_cached(
            &cache.join("dats").join(format!("{}.dat", system.id)),
            &crate::metadata::checksum_catalog_url(catalog, disc_catalog),
        ));
        let tree = read_cached(
            &cache.join("trees").join(format!("{}.json", system.id)),
            &artwork::artwork_index_url(catalog),
        );
        let filenames = artwork::filenames_from_git_tree(&tree)
            .unwrap_or_else(|error| panic!("{} picture list: {error}", system.id));
        let index = artwork::ArtworkIndex::from_filenames(&filenames);
        let mut row = Totals::default();
        for name in &names {
            let retail = artwork::is_retail_name(name);
            row.names += 1;
            row.retail += u64::from(retail);
            let matched = artwork::match_cover(&index, name);
            if let Some(found) = &matched {
                assert!(
                    artwork::same_game_title(name, &found.filename),
                    "{} handed {} the cover of {}",
                    system.id,
                    name,
                    found.filename
                );
                row.matched += 1;
                row.matched_retail += u64::from(retail);
                if found.rung == MatchRung::Exact {
                    row.exact += 1;
                    row.exact_retail += u64::from(retail);
                }
            } else if retail && misses.len() < 20 {
                misses.push(format!("{}  {}", system.id, name));
            }
            let pictured = artwork::title_has_any_picture(&index, name);
            let label = format!("{}  ", system.id);
            if retail
                && matched.is_none()
                && pictured
                && left.iter().filter(|line| line.starts_with(&label)).count() < 3
            {
                left.push(format!("{label}{name}"));
            }
            if pictured {
                row.ceiling += 1;
                row.ceiling_retail += u64::from(retail);
            }
        }
        println!(
            "{:<20} {:>7} {:>7} {:>7.1}% {:>7.1}% {:>7.1}%",
            system.id,
            row.names,
            row.retail,
            pct(row.exact_retail, row.retail),
            pct(row.matched_retail, row.retail),
            pct(row.ceiling_retail, row.retail)
        );
        if disc_catalog {
            disc.add(&row);
        } else {
            cartridge.add(&row);
        }
        if system.id == "atari5200" {
            assert_pointer_is_a_picture(catalog, &tree);
        }
    }

    print_group("cartridges", &cartridge);
    print_group("discs", &disc);
    println!("retail misses (sample):");
    for miss in &misses {
        println!("  {miss}");
    }
    println!("retail with a picture we refused (sample):");
    for item in &left {
        println!("  {item}");
    }

    let before = pct(cartridge.exact_retail, cartridge.retail);
    let after = pct(cartridge.matched_retail, cartridge.retail);
    let ceiling = pct(cartridge.ceiling_retail, cartridge.retail);
    assert!(
        (65.0..74.0).contains(&before),
        "the old exact-name rate should still be about 69 retail, was {before:.1}"
    );
    assert!(
        after + 0.05 >= 76.0,
        "cover rate {after:.1} is below the expected 78 retail"
    );
    assert!(
        after <= ceiling + 0.05,
        "cover rate {after:.1} is above the ceiling {ceiling:.1}, so some covers belong to other games"
    );
}

fn assert_pointer_is_a_picture(catalog: &str, tree: &[u8]) {
    let parsed: serde_json::Value = serde_json::from_slice(tree).unwrap();
    let pointer = parsed["tree"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["mode"].as_str() == Some("120000")
                && entry["path"].as_str().is_some_and(|path| {
                    path.starts_with("Named_Boxarts/") && path.ends_with(".png")
                })
        })
        .expect("Atari 5200 should still publish at least one pointer file");
    let path = pointer["path"].as_str().unwrap();
    let filename = path
        .trim_start_matches("Named_Boxarts/")
        .trim_end_matches(".png");
    let repository = catalog.replace(' ', "_");
    let raw = format!(
        "https://raw.githubusercontent.com/libretro-thumbnails/{}/master/{}",
        percent(&repository),
        path.split('/').map(percent).collect::<Vec<_>>().join("/")
    );
    let cdn = artwork::artwork_download_url(catalog, filename);
    let raw_body = get_prefix(&raw);
    let cdn_body = get_prefix(&cdn);
    assert!(
        !raw_body.starts_with(b"\x89PNG"),
        "GitHub returned a picture for a pointer file; the bug being fixed has gone"
    );
    assert!(
        cdn_body.starts_with(b"\x89PNG"),
        "the artwork host did not return a picture for {filename}"
    );
    println!("pointer check: {filename} is text on GitHub and a picture on the artwork host");
}

fn percent(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[derive(Default)]
struct Totals {
    names: u64,
    retail: u64,
    exact: u64,
    exact_retail: u64,
    matched: u64,
    matched_retail: u64,
    ceiling: u64,
    ceiling_retail: u64,
}

impl Totals {
    fn add(&mut self, other: &Totals) {
        self.names += other.names;
        self.retail += other.retail;
        self.exact += other.exact;
        self.exact_retail += other.exact_retail;
        self.matched += other.matched;
        self.matched_retail += other.matched_retail;
        self.ceiling += other.ceiling;
        self.ceiling_retail += other.ceiling_retail;
    }
}

fn print_group(label: &str, totals: &Totals) {
    println!(
        "{label}: {} names, {} retail. before {:.1}%  after {:.1}%  ceiling {:.1}%  (all-names after {:.1}%)",
        totals.names,
        totals.retail,
        pct(totals.exact_retail, totals.retail),
        pct(totals.matched_retail, totals.retail),
        pct(totals.ceiling_retail, totals.retail),
        pct(totals.matched, totals.names)
    );
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

fn distinct_names(bytes: &[u8]) -> BTreeSet<String> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("name \"")?;
            rest.strip_suffix('"').map(str::to_owned)
        })
        .collect()
}

/// Read the catalogues fetched earlier. We fetch them only when asked to.
///
/// Fetching on demand would make the measurement depend on a third-party web
/// API and its rate limits. Set `ROMINABOX_REFRESH_CATALOGUES=1` to refresh.
fn read_cached(path: &Path, url: &str) -> Vec<u8> {
    if let Ok(bytes) = fs::read(path) {
        if !bytes.is_empty() {
            return bytes;
        }
    }
    if std::env::var("ROMINABOX_REFRESH_CATALOGUES").is_err() {
        panic!(
            "no cached copy of {url}\n\
             at {}\n\n\
             The measurement runs offline, against catalogues and picture lists \
             fetched once. It does not call anyone's API while testing.\n\
             To fetch what is missing:\n\
             \x20 ROMINABOX_REFRESH_CATALOGUES=1 python3 scripts/test.py identification",
            path.display()
        );
    }
    eprintln!("fetching {url}");
    let bytes = get_all(url);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, &bytes).unwrap();
    bytes
}

fn get_all(url: &str) -> Vec<u8> {
    let mut last = String::new();
    for attempt in 1..=4 {
        match ureq::get(url)
            .set("User-Agent", "rominabox-identification")
            .timeout(Duration::from_secs(120))
            .call()
        {
            Ok(response) => {
                let mut bytes = Vec::new();
                response
                    .into_reader()
                    .take(64 * 1024 * 1024)
                    .read_to_end(&mut bytes)
                    .unwrap();
                return bytes;
            }
            Err(error) => {
                last = error.to_string();
                let retry = last.contains("500") || last.contains("502") || last.contains("503");
                if !retry || attempt == 4 {
                    break;
                }
                eprintln!("retry {attempt} after {url}: {last}");
                std::thread::sleep(Duration::from_secs(2 * attempt));
            }
        }
    }
    panic!("GET {url}: {last}");
}

fn get_prefix(url: &str) -> Vec<u8> {
    let response = ureq::get(url)
        .set("User-Agent", "rominabox-identification")
        .timeout(Duration::from_secs(30))
        .call()
        .unwrap_or_else(|error| panic!("GET {url}: {error}"));
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(64)
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}
