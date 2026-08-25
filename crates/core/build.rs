fn main() {
    prost_build::compile_protos(&["proto/cosync.proto"], &["proto/"])
        .expect("failed to compile proto/cosync.proto");

    if std::env::var_os("CARGO_FEATURE_MOBILE_BINDINGS").is_some() {
        uniffi::generate_scaffolding("src/cosync_mobile.udl")
            .expect("failed to generate UniFFI mobile scaffolding");
    }
}
