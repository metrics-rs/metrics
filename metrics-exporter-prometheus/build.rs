fn main() {
    #[cfg(feature = "protobuf")]
    {
        prost_build::compile_protos(&["proto/metrics.proto"], &["proto/"]).unwrap();
    }
}
