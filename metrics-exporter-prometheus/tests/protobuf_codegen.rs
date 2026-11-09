#![cfg(feature = "protobuf")]

use std::path::Path;

/// Regenerates the protobuf code with `protox` and checks it matches the checked-in copy.
///
/// Set `REGENERATE_PROTOBUF=1` to overwrite the checked-in copy instead.
#[test]
fn checked_in_protobuf_code_is_current() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("protobuf_codegen");
    std::fs::create_dir_all(&out_dir).unwrap();

    let file_descriptors =
        protox::compile([crate_dir.join("proto/metrics.proto")], [crate_dir.join("proto")])
            .unwrap();
    prost_build::Config::new().out_dir(&out_dir).compile_fds(file_descriptors).unwrap();

    let generated = std::fs::read_to_string(out_dir.join("io.prometheus.client.rs")).unwrap();
    let checked_in_path = crate_dir.join("src/protobuf/io.prometheus.client.rs");

    if std::env::var_os("REGENERATE_PROTOBUF").is_some() {
        std::fs::write(&checked_in_path, generated).unwrap();
        return;
    }

    let checked_in = std::fs::read_to_string(&checked_in_path).unwrap();
    assert!(
        generated == checked_in,
        "src/protobuf/io.prometheus.client.rs is out of date; rerun this test with REGENERATE_PROTOBUF=1"
    );
}
