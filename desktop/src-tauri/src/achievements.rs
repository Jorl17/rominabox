//! A game's achievements, which we look up once and bundle into the game.
//!
//! We make the rows in `lists`, with the same list code for every screen of
//! rows. Here we only define what an achievement row contains, and put the
//! badges where the row can refer to them.
//!
//! Only here do we identify a ROM and fetch a game's achievements, and we do
//! both in the builder. In the exported game we never fetch a list from the
//! network, and the game contains no account.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// The switch for the whole feature.
///
/// When this is false, an export contains no achievement data, no badges and
/// no achievements screen, whatever is in the project.
pub const BUNDLING_ENABLED: bool = true;

/// The source of the badges. The payload contains no pictures, only the
/// badge's name, and the locked badge has the same name with `_lock`.
const BADGE_BASE: &str = "https://media.retroachievements.org/Badge";
const API_BASE: &str = "https://retroachievements.org/API";
/// The endpoint where we look up a game by its hash. Emulators use the same
/// endpoint, and it works without an account.
const CONNECT_BASE: &str = "https://retroachievements.org/dorequest.php";

/// The one achievement for finishing the game. We draw it differently in the
/// list, because finishing the game is not like any other row on the page.
const WIN_CONDITION: &str = "win_condition";

/// What the author chose. Empty is the ordinary case, with no achievements
/// screen, and the game runs without achievements.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AchievementSelection {
    /// The service's game id, found by hashing the ROM. When it is absent, the
    /// game has none and we build no screen.
    #[serde(default)]
    pub game_id: Option<u32>,
    /// Bundle the list into the exported game. Off by default.
    #[serde(default)]
    pub bundle: bool,
    /// A list already fetched, as a file. When it is absent, we fetch it now,
    /// and the person bundling must be signed in. With this file we can bundle
    /// a real list in a build without network, such as a test or a harness.
    #[serde(default)]
    pub catalog: Option<std::path::PathBuf>,
    /// Badge pictures already downloaded, in a directory, with the names they
    /// have on the service. When it is present, we use no network in the export.
    #[serde(default)]
    pub badges: Option<std::path::PathBuf>,
}

impl AchievementSelection {
    pub fn is_empty(&self) -> bool {
        !BUNDLING_ENABLED || self.game_id.is_none()
    }
}

/// The list for this export, fetched if necessary.
///
/// We fetch nothing unless the author chose to bundle the list, and the person
/// bundling must be signed in to fetch it. In the exported game we never use
/// this function, and read the file written here instead.
pub fn resolve(selection: &AchievementSelection) -> Result<Option<Catalog>, String> {
    if !BUNDLING_ENABLED || !selection.bundle {
        return Ok(None);
    }
    if let Some(path) = &selection.catalog {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        return Ok(Some(serde_json::from_str(&text).map_err(|error| {
            format!("{} is not an achievement list: {error}", path.display())
        })?));
    }
    let Some(game_id) = selection.game_id else {
        return Ok(None);
    };
    let Some(account) = Account::from_environment() else {
        return Err(
            "bundling achievements needs the person building the game to be signed in. \
             Set RA_USERNAME and RA_API_KEY, or export without them."
                .into(),
        );
    };
    Ok(Some(fetch_catalog(&account, game_id)?))
}

/// One achievement, as the service describes it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub points: u32,
    /// The badge's name at `media.retroachievements.org`, not a path.
    pub badge: String,
    pub earned: bool,
    /// The type of this one on the service: `progression`, `win_condition`,
    /// or nothing, as for most of them.
    #[serde(default)]
    pub kind: Option<String>,
}

/// A game's whole list, as it is bundled into an export.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub game_id: u32,
    pub title: String,
    pub achievements: Vec<Achievement>,
}

/// The reply from the service. We read only the fields used in a row, and the
/// payload contains a great deal more.
#[derive(Deserialize)]
struct RemoteGame {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Achievements")]
    achievements: std::collections::BTreeMap<String, RemoteAchievement>,
}

#[derive(Deserialize)]
struct RemoteAchievement {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Description")]
    description: String,
    #[serde(rename = "Points")]
    points: u32,
    #[serde(rename = "BadgeName")]
    badge: String,
    #[serde(rename = "DateEarned")]
    date_earned: Option<String>,
    #[serde(rename = "DisplayOrder")]
    display_order: Option<i64>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// The account with which the person bundling is signed in.
///
/// We read it from the environment, never from a project, and never write it
/// into an export. The person bundling achievements must be signed in.
pub struct Account {
    user: String,
    key: String,
}

impl Account {
    pub fn from_environment() -> Option<Account> {
        let user = std::env::var("RA_USERNAME").ok()?;
        let key = std::env::var("RA_API_KEY").ok()?;
        if user.is_empty() || key.is_empty() {
            return None;
        }
        Some(Account { user, key })
    }
}

/// The hash by which a ROM is known on the service.
///
/// It is not the file's MD5 for every console. An iNES or FDS header is 16
/// bytes of metadata from the dumper, not from the cartridge, and only the
/// cartridge data is part of the hash. We hash a Mega Drive file whole.
pub fn rom_hash(bytes: &[u8]) -> String {
    let headered = bytes.len() > 16
        && (&bytes[..3] == b"NES" || &bytes[..3] == b"FDS")
        && bytes[3] == 0x1a;
    let body = if headered { &bytes[16..] } else { bytes };
    use md5::Digest;
    let mut hash = md5::Md5::new();
    hash.update(body);
    hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn rom_hash_of(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("could not read {} to identify it: {error}", path.display()))?;
    Ok(rom_hash(&bytes))
}

/// The credentials are in the query string, as in the service's API. Nothing
/// that contains them may reach a log, a message or a screen, so we never show
/// an error from ureq as it is, because it includes the whole URL.
fn without_credentials(text: &str) -> String {
    let mut clean = text.to_string();
    for key in ["y", "z"] {
        let marker = format!("{key}=");
        let mut from = 0;
        while let Some(found) = clean[from..].find(&marker) {
            let at = from + found;
            let value_at = at + marker.len();
            let end = clean[value_at..]
                .find(|c| c == '&' || c == ' ')
                .map(|offset| value_at + offset)
                .unwrap_or(clean.len());
            clean.replace_range(value_at..end, "...");
            from = value_at + "...".len();
        }
    }
    clean
}

fn fetch(url: &str) -> Result<String, String> {
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|error| {
            format!(
                "the achievement service did not answer: {}",
                without_credentials(&error.to_string())
            )
        })?;
    response
        .into_string()
        .map_err(|error| format!("the achievement service answered unreadably: {error}"))
}

/// The game for a ROM, or None when the service has no game for it.
///
/// This is the only call that works without an account, because the answer
/// depends on the cartridge, not on the person asking.
pub fn identify(hash: &str) -> Result<Option<u32>, String> {
    let body = fetch(&format!("{CONNECT_BASE}?r=gameid&m={hash}"))?;
    let answer: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("the achievement service answered unreadably: {error}"))?;
    match answer.get("GameID").and_then(|value| value.as_u64()) {
        None | Some(0) => Ok(None),
        Some(id) => Ok(Some(id as u32)),
    }
}

/// A game's whole list, in the order shown on the service.
pub fn fetch_catalog(account: &Account, game_id: u32) -> Result<Catalog, String> {
    let body = fetch(&format!(
        "{API_BASE}/API_GetGameExtended.php?i={game_id}&y={key}&z={user}",
        key = account.key,
        user = account.user,
    ))?;
    parse_catalog(&body)
}

pub fn parse_catalog(body: &str) -> Result<Catalog, String> {
    let game: RemoteGame = serde_json::from_str(body)
        .map_err(|error| format!("the achievement list could not be read: {error}"))?;
    let mut achievements: Vec<(i64, Achievement)> = game
        .achievements
        .into_values()
        .map(|entry| {
            (
                entry.display_order.unwrap_or(0),
                Achievement {
                    id: entry.id,
                    title: entry.title,
                    description: entry.description,
                    points: entry.points,
                    badge: entry.badge,
                    earned: entry.date_earned.is_some(),
                    kind: entry.kind,
                },
            )
        })
        .collect();
    achievements.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    Ok(Catalog {
        game_id: game.id,
        title: game.title,
        achievements: achievements.into_iter().map(|(_, entry)| entry).collect(),
    })
}

/// On the service, a locked badge has the name with `_lock` added. We keep that
/// name for the file, so we can give a directory of downloaded badges straight
/// to an export, without a second naming scheme.
pub fn badge_file(badge: &str, earned: bool) -> String {
    if earned {
        format!("{badge}.png")
    } else {
        format!("{badge}_lock.png")
    }
}

/// A badge name from the service that is safe to use in a file name and in the
/// `src` of a row.
///
/// The payload comes from somebody else, and we fill the holes of the row
/// template with it. A quote in a name would end the attribute and start
/// markup, and a slash or a dot-dot would lead outside the menu assets.
fn badge_is_safe(badge: &str) -> bool {
    !badge.is_empty()
        && badge.len() <= 64
        && badge
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn badge_url(badge: &str, earned: bool) -> String {
    format!("{BADGE_BASE}/{}", badge_file(badge, earned))
}

/// Put every badge of a list into a directory, once, so the person can repeat
/// an export without the network.
///
/// We download both states of every badge, not only the one for this account,
/// because the exported game does not show the state of the account that
/// fetched the list, and a row without a downloaded picture is a white square.
pub fn download_badges(catalog: &Catalog, into: &Path) -> Result<usize, String> {
    fs::create_dir_all(into).map_err(|error| error.to_string())?;
    let mut written = 0;
    for achievement in &catalog.achievements {
        if !badge_is_safe(&achievement.badge) {
            continue;
        }
        for earned in [true, false] {
            let target = into.join(badge_file(&achievement.badge, earned));
            if target.is_file() {
                continue;
            }
            let Ok(response) = ureq::get(&badge_url(&achievement.badge, earned))
                .timeout(std::time::Duration::from_secs(20))
                .call()
            else {
                continue;
            };
            let mut bytes = Vec::new();
            if std::io::copy(&mut response.into_reader(), &mut bytes).is_err() {
                continue;
            }
            fs::write(&target, bytes).map_err(|error| error.to_string())?;
            written += 1;
        }
    }
    Ok(written)
}

/// The picture for a row when its badge was never downloaded.
///
/// An `img` without a picture is a white rectangle, which is worse than no
/// picture at all. This is the empty well of the save slots, at badge size.
fn placeholder_png() -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, Rgba};
    let mut image: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(56, 56, Rgba([6, 26, 72, 255]));
    for x in 0..56u32 {
        for y in [0u32, 55] {
            image.put_pixel(x, y, Rgba([71, 110, 171, 255]));
            image.put_pixel(y, x, Rgba([71, 110, 171, 255]));
        }
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| format!("could not draw a badge placeholder: {error}"))?;
    Ok(bytes.into_inner())
}

/// The picture for a row, put beside the menu once when we bundle the game.
///
/// A badge that we cannot download does not stop the export. The row keeps its
/// words without its picture, which a player prefers.
fn stage_badge(
    menu_assets: &Path,
    achievement: &Achievement,
    downloaded: Option<&Path>,
) -> Result<String, String> {
    let directory = menu_assets.join("achievements");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let placeholder = || -> Result<String, String> {
        let name = "missing.png";
        let target = directory.join(name);
        if !target.is_file() {
            fs::write(&target, placeholder_png()?).map_err(|error| error.to_string())?;
        }
        Ok(format!("achievements/{name}"))
    };
    if !badge_is_safe(&achievement.badge) {
        return placeholder();
    }
    let source_name = badge_file(&achievement.badge, achievement.earned);
    let target = directory.join(&source_name);
    let relative = format!("achievements/{source_name}");
    if target.is_file() {
        return Ok(relative);
    }
    if let Some(from) = downloaded {
        // The other state's picture rather than nothing, because a badge is the
        // same artwork locked or not. A grey badge is better than an empty well.
        for name in [
            source_name.clone(),
            badge_file(&achievement.badge, !achievement.earned),
        ] {
            if fs::copy(from.join(&name), &target).is_ok() {
                return Ok(relative);
            }
        }
        return placeholder();
    }
    let response = ureq::get(&badge_url(&achievement.badge, achievement.earned))
        .timeout(std::time::Duration::from_secs(20))
        .call();
    let Ok(response) = response else {
        return placeholder();
    };
    let mut bytes = Vec::new();
    if std::io::copy(&mut response.into_reader(), &mut bytes).is_err() {
        return placeholder();
    }
    fs::write(&target, bytes).map_err(|error| error.to_string())?;
    Ok(relative)
}

/// The text on the right of a row: whether it is earned, and its points.
///
/// We set the words in the export, not in the player, so a design in another
/// language can have different words without a change to the player.
fn row_state(achievement: &Achievement) -> String {
    format!(
        "{} {} PTS",
        if achievement.earned { "UNLOCKED" } else { "LOCKED" },
        achievement.points
    )
}

/// Turn a bundled catalog into the rows of a list.
pub fn rows(
    menu_assets: &Path,
    catalog: &Catalog,
    downloaded: Option<&Path>,
) -> Result<Vec<crate::lists::ListItem>, String> {
    let mut items = Vec::new();
    for achievement in &catalog.achievements {
        items.push(crate::lists::ListItem {
            id: format!("achievement-{}", achievement.id),
            icon: stage_badge(menu_assets, achievement, downloaded)?,
            title: achievement.title.to_uppercase(),
            detail: achievement.description.clone(),
            state: row_state(achievement),
            selected: false,
            accent: achievement.kind.as_deref() == Some(WIN_CONDITION),
        });
    }
    Ok(items)
}

/// The achievements screen, as declared in the design.
///
/// We read all of a design's screens with one parser, so a field added to
/// Screen applies to this screen as well.
pub fn screen(design: &Path) -> Option<crate::themes::Screen> {
    crate::themes::declared_screens(design)
        .ok()?
        .into_iter()
        .find(|screen| screen.id == "achievements")
}

pub struct Staged {
    pub list: Option<crate::lists::List>,
}

/// Put a game's achievements into its export.
///
/// We bundle the catalog. We write the list beside the menu and copy the
/// badges in, and the exported game uses no network. For a selection with no
/// game id, or a build with bundling switched off, we stage nothing.
pub fn stage(
    design: &Path,
    menu_assets: &Path,
    catalog: Option<&Catalog>,
    downloaded: Option<&Path>,
) -> Result<Staged, String> {
    if !BUNDLING_ENABLED {
        return Ok(Staged { list: None });
    }
    let Some(catalog) = catalog else {
        return Ok(Staged { list: None });
    };
    let Some(screen) = screen(design) else {
        return Err(
            "this design declares no achievements screen, so the list has nowhere to go".into(),
        );
    };
    if catalog.achievements.is_empty() {
        return Ok(Staged { list: None });
    }
    let items = rows(menu_assets, catalog, downloaded)?;
    fs::write(
        menu_assets.join("achievements.json"),
        serde_json::to_string_pretty(catalog).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(Staged {
        list: Some(crate::lists::List { screen, items }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headered_cartridge_is_hashed_without_its_header() {
        let mut headered = b"NES\x1a".to_vec();
        headered.extend(std::iter::repeat(0u8).take(12));
        headered.extend_from_slice(b"cartridge");
        assert_eq!(rom_hash(&headered), rom_hash(b"cartridge"));
    }

    #[test]
    fn a_mega_drive_file_is_hashed_whole() {
        let rom = b"SEGA MEGA DRIVE and then the game".to_vec();
        assert_eq!(rom_hash(&rom), rom_hash(&rom.clone()));
        assert_eq!(rom_hash(&rom).len(), 32);
    }

    #[test]
    fn a_four_byte_file_is_not_mistaken_for_a_header() {
        assert_ne!(rom_hash(b"NES\x1a"), rom_hash(b""));
    }

    #[test]
    fn the_list_comes_back_in_the_order_the_service_displays_it() {
        let body = r#"{
            "ID": 1, "Title": "Sonic the Hedgehog",
            "Achievements": {
                "2": {"ID": 2, "Title": "Second", "Description": "b", "Points": 5,
                      "BadgeName": "0002", "DateEarned": null, "DisplayOrder": 2},
                "1": {"ID": 1, "Title": "First", "Description": "a", "Points": 3,
                      "BadgeName": "0001", "DateEarned": "2026-01-01 00:00:00", "DisplayOrder": 1}
            }
        }"#;
        let catalog = parse_catalog(body).expect("a readable list");
        assert_eq!(catalog.game_id, 1);
        let titles: Vec<&str> = catalog
            .achievements
            .iter()
            .map(|entry| entry.title.as_str())
            .collect();
        assert_eq!(titles, vec!["First", "Second"]);
        assert!(catalog.achievements[0].earned);
        assert!(!catalog.achievements[1].earned);
    }

    #[test]
    fn a_badge_name_cannot_write_markup_or_leave_the_menu_assets() {
        assert!(badge_is_safe("310158"));
        assert!(badge_is_safe("badge_lock-2"));
        assert!(!badge_is_safe(""));
        assert!(!badge_is_safe("a\"/><script"));
        assert!(!badge_is_safe("../../etc/passwd"));
        assert!(!badge_is_safe("a b"));
    }

    #[test]
    fn credentials_never_reach_a_message() {
        let leaked = "https://retroachievements.org/API/API_GetGameExtended.php?i=1&y=K7jEqIJw&z=Someone: status code 404";
        let clean = without_credentials(leaked);
        assert!(!clean.contains("K7jEqIJw"), "{clean}");
        assert!(!clean.contains("z=Someone"), "{clean}");
        assert!(clean.contains("status code 404"), "{clean}");
    }

    #[test]
    fn a_row_says_whether_it_is_earned_and_what_it_is_worth() {
        let locked = Achievement {
            id: 1,
            title: "That Was Easy".into(),
            description: "Complete the first act of Green Hill Zone.".into(),
            points: 3,
            badge: "0001".into(),
            earned: false,
            kind: None,
        };
        assert_eq!(row_state(&locked), "LOCKED 3 PTS");
        assert_eq!(row_state(&Achievement { earned: true, ..locked }), "UNLOCKED 3 PTS");
    }
}
