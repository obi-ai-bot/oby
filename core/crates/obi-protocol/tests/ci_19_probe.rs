// Temporary (R01-02, CI-19): fails on its first attempt only, so the ci profile's retry passes it
// and Check lists it as flaky. The marker in the temporary folder outlives the first attempt.
#[test]
fn ci_19_probe_fails_on_its_first_attempt_only() {
    let marker = std::env::temp_dir().join("oby-ci-19-probe");
    if !marker.exists() {
        std::fs::write(&marker, "").unwrap();
        panic!("CI-19 probe: first attempt");
    }
}
