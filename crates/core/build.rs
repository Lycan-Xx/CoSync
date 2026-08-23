fn main() {
    prost_build::compile_protos(&["proto/cosync.proto"], &["proto/"])
        .expect("failed to compile proto/cosync.proto — is protoc installed?");
}
