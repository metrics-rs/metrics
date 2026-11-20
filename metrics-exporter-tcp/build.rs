fn main() {
    println!("cargo:rerun-if-changed=proto/event.proto");
    let file_descriptors = protox::compile(["proto/event.proto"], ["proto/"]).unwrap();
    let mut prost_build = prost_build::Config::new();
    prost_build.btree_map(["."]);
    prost_build.compile_fds(file_descriptors).unwrap();
}
