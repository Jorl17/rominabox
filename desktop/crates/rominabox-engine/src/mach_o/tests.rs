//! Tests of reading, joining, renaming and signing Mach-O files. We compare
//! each result with the output of the Apple tools `lipo`, `otool`,
//! `install_name_tool` and `codesign` for the same files, on a Mac only. We
//! never use those tools in an export. We check the DER of the entitlements
//! on every platform, against bytes written by `codesign`.

use super::entitlements::{Entitlements, Value};

/// The entitlements, in DER from `codesign`, of a game signed with it (Super
/// Mario Bros. 3, with achievements): `70 82 01 c3 ...`.
#[test]
fn entitlements_are_written_in_der_as_codesign_writes_them() {
    let entitlements = Entitlements::default()
        .with("com.apple.security.app-sandbox", Value::Bool(true))
        .with("com.apple.security.network.client", Value::Bool(true))
        .with("com.apple.security.device.usb", Value::Bool(true))
        .with("com.apple.security.device.bluetooth", Value::Bool(true))
        .with(
            "com.apple.security.temporary-exception.files.home-relative-path.read-only",
            Value::Strings(vec![
                "/Library/Application Support/ROM-in-a-Box/Games/65bf5bd3b4c76232815303b0/".into(),
            ]),
        )
        .with(
            "com.apple.security.temporary-exception.files.home-relative-path.read-write",
            Value::Strings(vec![
                "/Library/Application Support/ROM-in-a-Box Accounts/".into()
            ]),
        );
    let written = concat!(
        "708201c3020101b08201bc30230c1e636f6d2e6170706c652e73656375726974792e6170702d73616e64626f780101ff",
        "30280c23636f6d2e6170706c652e73656375726974792e6465766963652e626c7565746f6f74680101ff30220c1d636f",
        "6d2e6170706c652e73656375726974792e6465766963652e7573620101ff30260c21636f6d2e6170706c652e73656375",
        "726974792e6e6574776f726b2e636c69656e740101ff3081980c49636f6d2e6170706c652e73656375726974792e7465",
        "6d706f726172792d657863657074696f6e2e66696c65732e686f6d652d72656c61746976652d706174682e726561642d",
        "6f6e6c79304b0c492f4c6962726172792f4170706c69636174696f6e20537570706f72742f524f4d2d696e2d612d426f",
        "782f47616d65732f3635626635626433623463373632333238313533303362302f3081830c4a636f6d2e6170706c652e",
        "73656375726974792e74656d706f726172792d657863657074696f6e2e66696c65732e686f6d652d72656c6174697665",
        "2d706174682e726561642d777269746530350c332f4c6962726172792f4170706c69636174696f6e20537570706f7274",
        "2f524f4d2d696e2d612d426f78204163636f756e74732f",
    );
    let hex: String = entitlements
        .der()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(hex, written);
}

#[cfg(target_os = "macos")]
mod against_apples_tools;
