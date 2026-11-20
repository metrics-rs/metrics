fn main() {
    #[cfg(feature = "protobuf")]
    {
        let file_descriptors = protox::compile(["proto/metrics.proto"], ["proto/"]).unwrap();
        prost_build::Config::new().compile_fds(file_descriptors).unwrap();
    }
}
