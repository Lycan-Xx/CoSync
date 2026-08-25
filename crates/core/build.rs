fn main() {
    prost_build::compile_protos(&["proto/cosync.proto"], &["proto/"])
        .expect("failed to compile proto/cosync.proto");

    #[cfg(feature = "mobile-bindings")]
    {
        uniffi::generate_scaffolding("src/cosync_mobile.udl")
            .expect("failed to generate UniFFI mobile scaffolding");
    }
}
