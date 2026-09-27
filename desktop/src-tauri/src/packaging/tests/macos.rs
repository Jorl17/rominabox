use super::*;
use crate::packaging::macos::compile_c;

#[test]
fn concurrent_launcher_compiles_publish_without_sharing_temporary_files() {
    let root = rominabox_scratch::Scratch::dir("rominabox-compile-concurrency");
    let source = root.join("fixture.c");
    let output = root.join("fixture.o");
    std::fs::write(&source, "int fixture(void) { return 42; }\n").unwrap();
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|threads| {
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                threads.spawn(|| {
                    barrier.wait();
                    compile_c(std::slice::from_ref(&source), &output, &["-c"])
                })
            })
            .collect();
        for job in jobs {
            job.join()
                .unwrap()
                .expect("each concurrent compile publishes safely");
        }
    });
    assert!(output.is_file());
}

/// The launcher includes declarations from the player tree. We rebuild it
/// after a change to one of them, though that file is not among its inputs.
#[test]
#[cfg_attr(windows, ignore = "the export-time launcher compile is macOS-only")]
fn a_header_outside_the_inputs_rebuilds_what_includes_it() {
    let root = rominabox_scratch::Scratch::dir("rominabox-compile-includes");
    let (folder, shared) = (root.join("launcher"), root.join("shared"));
    fs::create_dir_all(&folder).unwrap();
    fs::create_dir_all(&shared).unwrap();
    let (source, header) = (folder.join("fixture.c"), shared.join("value.h"));
    let output = root.join("fixture.o");
    fs::write(&header, "#define VALUE 1\n").unwrap();
    fs::write(
        &source,
        "#include \"../shared/value.h\"\nint fixture(void) { return VALUE; }\n",
    )
    .unwrap();
    compile_c(std::slice::from_ref(&source), &output, &["-c"]).unwrap();
    let first = fs::read(&output).unwrap();

    fs::write(&header, "#define VALUE 2\n").unwrap();
    let later =
        fs::metadata(&output).unwrap().modified().unwrap() + std::time::Duration::from_secs(2);
    fs::File::options()
        .write(true)
        .open(&header)
        .unwrap()
        .set_modified(later)
        .unwrap();
    compile_c(std::slice::from_ref(&source), &output, &["-c"]).unwrap();
    assert_ne!(
        fs::read(&output).unwrap(),
        first,
        "the object holds the new value"
    );
}
