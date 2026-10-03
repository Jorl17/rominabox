//! We read a compressed CD image as its tracks, named as in Redump, with
//! their exact size, and write them out byte for byte as they went in, with
//! audio back in little-endian, and a cue sheet with each track's pregap.

use std::fs;

use rominabox_engine::chd_disc::{self, TrackKind};
use rominabox_scratch::Scratch;

mod chd_writer;
use chd_writer::{write_cd, CdTrack};

/// `count` sectors of `size` bytes, each different from the others.
fn sectors(count: usize, size: usize, seed: u8) -> Vec<u8> {
    (0..count * size).map(|at| (at / 7) as u8 ^ seed ^ (at % 251) as u8).collect()
}

#[test]
fn a_cd_image_of_several_tracks_is_read_and_written_out_as_it_went_in() {
    let root = Scratch::dir("rominabox-chd-tracks");
    let data = sectors(10, 2352, 1);
    let first_audio = sectors(5, 2352, 2);
    let second_audio = sectors(3, 2352, 3);
    let chd = root.join("Tiny Disc (Europe).chd");
    write_cd(
        &chd,
        &[
            CdTrack { kind: "MODE2_RAW", bytes: &data, pregap: 0 },
            CdTrack { kind: "AUDIO", bytes: &first_audio, pregap: 2 },
            CdTrack { kind: "AUDIO", bytes: &second_audio, pregap: 0 },
        ],
    );

    let disc = chd_disc::read(&chd, "Tiny Disc (Europe)").unwrap();
    let names: Vec<&str> = disc.tracks.iter().map(|track| track.name.as_str()).collect();
    assert_eq!(
        names,
        ["Tiny Disc (Europe) (Track 1).bin", "Tiny Disc (Europe) (Track 2).bin", "Tiny Disc (Europe) (Track 3).bin"]
    );
    assert_eq!(disc.tracks[0].kind, TrackKind::Mode2Raw);
    assert_eq!(disc.bytes(), (data.len() + first_audio.len() + second_audio.len()) as u64);
    for (track, expected) in disc.tracks.iter().zip([&data, &first_audio, &second_audio]) {
        let mut written = Vec::new();
        chd_disc::write_track(&chd, track, &mut written).unwrap();
        assert!(written == **expected, "{} is not what went in", track.name);
        assert_eq!(chd_disc::track_crc(&chd, track).unwrap(), crc32fast::hash(expected));
    }
    assert_eq!(
        disc.sheet(),
        "FILE \"Tiny Disc (Europe) (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n\
         FILE \"Tiny Disc (Europe) (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:00:02\n\
         FILE \"Tiny Disc (Europe) (Track 3).bin\" BINARY\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n"
    );
    assert_eq!(disc.sheet_name, "Tiny Disc (Europe).cue");
}

#[test]
fn a_disc_of_one_track_is_one_file_under_the_disc_s_name() {
    let root = Scratch::dir("rominabox-chd-one-track");
    let data = sectors(6, 2048, 9);
    let chd = root.join("Tiny Disc.chd");
    write_cd(&chd, &[CdTrack { kind: "MODE1", bytes: &data, pregap: 0 }]);

    let disc = chd_disc::read(&chd, "Tiny Disc").unwrap();
    assert_eq!(disc.tracks.len(), 1);
    assert_eq!(disc.tracks[0].name, "Tiny Disc.bin");
    assert_eq!(disc.sheet(), "FILE \"Tiny Disc.bin\" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n");
    let mut written = Vec::new();
    chd_disc::write_track(&chd, &disc.tracks[0], &mut written).unwrap();
    assert!(written == data);
}

#[test]
fn a_file_that_is_not_a_chd_is_refused() {
    let root = Scratch::dir("rominabox-chd-not");
    let fake = root.join("Fake.chd");
    fs::write(&fake, b"not a chd at all").unwrap();
    assert!(chd_disc::read(&fake, "Fake").is_err());
}
