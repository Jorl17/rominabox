//! Applying every patch in the corpus as in an export gives the game it was
//! made for: we apply it to a game file, into a new file, both mapped from
//! disk (patching::apply_files). We refuse a patch for another revision of
//! its game.
//!
//! We write the corpus (tests/fixtures/patches/corpus) with
//! `scripts/patch_corpus.py`: the edge cases of each format, IPS, UPS, BPS
//! and xdelta, with each xdelta result as the xdelta3 command decodes it. In
//! the cross-check of that script, we run this test on a larger, random
//! corpus through ROMINABOX_PATCH_CORPUS.

use std::fs;
use std::path::{Path, PathBuf};

use rominabox_engine::patching::{self, FileFailure};
use rominabox_engine::repo;
use rominabox_scratch::Scratch;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    patch: String,
    seed: u64,
    size: usize,
    /// For a patch applied to another game: the byte of the game that differs.
    flip: Option<usize>,
    #[serde(default)]
    refused: bool,
    target: Option<Target>,
}

#[derive(Deserialize)]
struct Target {
    size: u64,
    sha256: String,
}

/// The output of splitmix64, eight little-endian bytes a step, as we make
/// each case's game in the script.
fn game(seed: u64, size: usize) -> Vec<u8> {
    let mut state = seed;
    let mut bytes = Vec::with_capacity(size + 8);
    while bytes.len() < size {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        bytes.extend(z.to_le_bytes());
    }
    bytes.truncate(size);
    bytes
}

fn corpus_folder() -> PathBuf {
    std::env::var_os("ROMINABOX_PATCH_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo::root().join("desktop/crates/rominabox-engine/tests/fixtures/patches/corpus"))
}

/// The result of applying the case's patch, compared with what it should be.
fn check(case: &Case, folder: &Path, scratch: &Path) -> Result<(), String> {
    let mut original = game(case.seed, case.size);
    if let Some(at) = case.flip {
        original[at] ^= 0xFF;
    }
    let source = scratch.join("game");
    let made = scratch.join("made");
    fs::write(&source, &original).unwrap();
    let _ = fs::remove_file(&made);
    let patch = folder.join(&case.patch);
    let applied = patching::apply_files(&source, &[patch.as_path()], &made);
    match (applied, &case.target) {
        (Err((_, FileFailure::Patch(_))), _) if case.refused => Ok(()),
        (Ok(()), _) if case.refused => Err("applied to another game".into()),
        (Ok(()), Some(target)) => {
            let bytes = fs::read(&made).unwrap();
            let digest = format!("{:x}", Sha256::digest(&bytes));
            if bytes.len() as u64 == target.size && digest == target.sha256 {
                Ok(())
            } else {
                Err(format!("made {} bytes, {digest}; wanted {} bytes, {}", bytes.len(), target.size, target.sha256))
            }
        }
        (Ok(()), None) => Err("the corpus states no target".into()),
        (Err((_, failure)), _) => Err(format!("refused: {failure:?}")),
    }
}

#[test]
fn every_patch_in_the_corpus_makes_the_game_it_was_made_to_make() {
    let folder = corpus_folder();
    let corpus: Corpus = serde_json::from_slice(&fs::read(folder.join("corpus.json")).unwrap()).unwrap();
    let scratch = Scratch::dir("rominabox-patch-corpus");
    let failures: Vec<String> = corpus
        .cases
        .iter()
        .filter_map(|case| check(case, &folder, &scratch).err().map(|why| format!("{}: {why}", case.patch)))
        .collect();
    let refused = corpus.cases.iter().filter(|case| case.refused).count();
    println!("{} patches, {refused} of them given another game", corpus.cases.len());
    assert!(corpus.cases.len() >= 50, "the corpus is the edges of every format");
    assert!(failures.is_empty(), "{} of {} failed:\n{}", failures.len(), corpus.cases.len(), failures.join("\n"));
}
