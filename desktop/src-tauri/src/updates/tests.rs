use super::*;

fn names(processors: &[&str]) -> Vec<String> {
    processors.iter().map(|name| name.to_string()).collect()
}

#[test]
fn each_builder_has_its_own_key_and_a_universal_one_stays_universal() {
    assert_eq!(platform_key("macos", &names(&["arm64"])), Ok(Some("darwin-aarch64".into())));
    assert_eq!(platform_key("macos", &names(&["x86_64"])), Ok(Some("darwin-x86_64".into())));
    assert_eq!(platform_key("macos", &names(&["arm64", "x86_64"])), Ok(Some("darwin-universal".into())));
    assert_eq!(platform_key("windows", &names(&["x86_64"])), Ok(Some("windows-x86_64".into())));
    assert_eq!(platform_key("windows", &names(&["aarch64"])), Ok(None));
}

/// A Mach-O file with the header alone, for `cpu`.
fn header_for(cpu: rominabox_engine::mach_o::Cpu) -> Vec<u8> {
    const MH_MAGIC_64: u32 = 0xfeed_facf;
    const MH_DYLIB: u32 = 6;
    [MH_MAGIC_64, cpu.kind, cpu.subtype, MH_DYLIB, 0, 0, 0, 0]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

#[test]
fn the_processors_come_from_the_code_in_the_program() {
    let arm = header_for(rominabox_engine::mach_o::Cpu::ARM64);
    let intel = header_for(rominabox_engine::mach_o::Cpu::X86_64);
    let both = rominabox_engine::mach_o::join(&[&arm, &intel]).unwrap();
    assert_eq!(mach_o_processors(&arm), Ok(names(&["arm64"])));
    assert_eq!(mach_o_processors(&both), Ok(names(&["arm64", "x86_64"])));
}

#[test]
fn we_look_on_the_website_first_and_then_on_github() {
    assert_eq!(
        manifest_addresses(),
        [
            "https://www.rominabox.app/updates/latest.json",
            "https://github.com/Jorl17/rominabox/releases/latest/download/latest.json"
        ]
    );
}

/// A web server on 127.0.0.1 that answers each path in `routes` with its
/// status and body, and every other path with 404.
fn serve(routes: Vec<(&'static str, u16, Vec<u8>)>) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap_or_default();
            // The rest of the request: its headers, up to the empty line.
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                line.clear();
            }
            let path = request.split_whitespace().nth(1).unwrap_or_default().to_string();
            let (status, body) = routes
                .iter()
                .find(|(route, _, _)| *route == path)
                .map(|(_, status, body)| (*status, body.clone()))
                .unwrap_or((404, b"missing".to_vec()));
            let mut stream = &stream;
            let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(&body);
        }
    });
    address
}

/// A key pair made for one test: the public key as the builder reads it,
/// and a function that signs data for a version, as `tauri signer sign
/// --app-version` does, or with no version for an empty one.
fn key_pair() -> (String, impl Fn(&[u8], &str) -> String) {
    use base64::Engine;
    let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let public = base64::engine::general_purpose::STANDARD.encode(pair.pk.to_box().unwrap().to_string());
    let sign = move |data: &[u8], version: &str| {
        let comment = match version {
            "" => "timestamp:0\tfile:package".to_string(),
            version => format!("timestamp:0\tfile:package\tversion:{version}"),
        };
        let signature =
            minisign::sign(Some(&pair.pk), &pair.sk, std::io::Cursor::new(data), Some(&comment), None).unwrap();
        base64::engine::general_purpose::STANDARD.encode(signature.to_string())
    };
    (public, sign)
}

/// A builder with no window, version 0.1.0, which trusts `public` and, for
/// these tests alone, reads manifests over http.
fn builder_trusting(public: &str) -> tauri::App<tauri::test::MockRuntime> {
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({ "pubkey": public, "requireSignedVersion": true, "dangerousInsecureTransportProtocol": true }),
    );
    tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap()
}

fn manifest(version: &str, package: &str, signature: &str) -> Vec<u8> {
    serde_json::json!({
        "version": version,
        "platforms": { "windows-x86_64": { "url": package, "signature": signature } },
    })
    .to_string()
    .into_bytes()
}

const PACKAGE: &[u8] = b"the newer builder";

/// What we find and download from a manifest at `second`, after `first`
/// answers with an error.
fn check_and_download(
    public: &str,
    first: &str,
    second: &str,
) -> (Option<String>, Option<Result<Vec<u8>, String>>) {
    let app = builder_trusting(public);
    let updater = updater(app.handle(), &[first.to_string(), second.to_string()], "windows-x86_64").unwrap();
    tauri::async_runtime::block_on(async move {
        let Some(update) = updater.check().await.unwrap() else {
            return (None, None);
        };
        let downloaded = update.download(|_, _| {}, || {}).await.map_err(|error| error.to_string());
        (Some(update.version.clone()), Some(downloaded))
    })
}

#[test]
fn a_newer_builder_signed_for_its_version_is_found_on_the_second_address_and_downloaded() {
    let (public, sign) = key_pair();
    let packages = serve(vec![("/package", 200, PACKAGE.to_vec())]);
    let good = serve(vec![(
        "/latest.json",
        200,
        manifest("0.2.0", &format!("{packages}/package"), &sign(PACKAGE, "0.2.0")),
    )]);
    let broken = serve(vec![("/latest.json", 500, b"down".to_vec())]);

    let (version, downloaded) =
        check_and_download(&public, &format!("{broken}/latest.json"), &format!("{good}/latest.json"));

    assert_eq!(version.as_deref(), Some("0.2.0"));
    assert_eq!(downloaded, Some(Ok(PACKAGE.to_vec())));
}

#[test]
fn a_package_whose_signature_does_not_match_is_refused() {
    let (public, sign) = key_pair();
    let (_, other_key) = key_pair();
    let packages = serve(vec![("/package", 200, PACKAGE.to_vec())]);
    for (case, signature) in [
        ("signed for other data", sign(b"another package", "0.2.0")),
        ("signed with another key", other_key(PACKAGE, "0.2.0")),
        ("signed for an older version", sign(PACKAGE, "0.1.5")),
        ("signed with no version", sign(PACKAGE, "")),
    ] {
        let site = serve(vec![("/latest.json", 200, manifest("0.2.0", &format!("{packages}/package"), &signature))]);
        let (version, downloaded) = check_and_download(&public, &format!("{site}/latest.json"), &format!("{site}/none"));
        assert_eq!(version.as_deref(), Some("0.2.0"), "{case}");
        assert!(downloaded.is_some_and(|result| result.is_err()), "{case}");
    }
}

#[test]
fn a_manifest_with_the_same_or_an_older_version_offers_nothing() {
    let (public, sign) = key_pair();
    for version in ["0.1.0", "0.0.9"] {
        let site = serve(vec![("/latest.json", 200, manifest(version, "http://127.0.0.1:1/package", &sign(PACKAGE, version)))]);
        let (found, _) = check_and_download(&public, &format!("{site}/latest.json"), &format!("{site}/none"));
        assert_eq!(found, None, "{version}");
    }
}
