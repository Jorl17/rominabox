//! We write the generated files into the checkout where someone runs the
//! generator. Checkouts can share one cargo target, so the binary may come
//! from another checkout, and the path compiled into it is that one's.
//!
//! This file has one test, alone in its binary, because in it we change
//! the working directory and environment of the process.

#[test]
fn the_repository_is_the_checkout_the_generator_is_run_from() {
    let checkout = rominabox_scratch::Scratch::dir("rominabox-catalog-here");
    std::fs::create_dir_all(checkout.join("integrations/consoles")).unwrap();
    std::fs::create_dir_all(checkout.join("desktop")).unwrap();
    std::env::remove_var("ROMINABOX_REPO");
    let started_in = std::env::current_dir().unwrap();
    std::env::set_current_dir(checkout.join("desktop")).unwrap();
    let found = rominabox_catalog::repo_root().canonicalize().unwrap();
    // On Windows we cannot remove a folder that a process has as its working
    // directory, so we leave the scratch checkout before we remove it.
    std::env::set_current_dir(started_in).unwrap();
    assert_eq!(found, checkout.to_path_buf().canonicalize().unwrap());
}
