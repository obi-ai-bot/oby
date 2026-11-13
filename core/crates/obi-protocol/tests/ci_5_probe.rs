// Temporary (R01-02, CI-5): a test that always fails, so a rust-test shard and Check go red.
#[test]
fn ci_5_probe_always_fails() {
    panic!("CI-5 probe: this test always fails");
}
