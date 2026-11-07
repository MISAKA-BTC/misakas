fn main() {
    // Build without a system `protoc`: point prost / tonic-prost-build at a vendored
    // protoc binary (protoc-bin-vendored ships one per host target — Windows/macOS/
    // Linux), so no separate protoc install is needed. A pre-set `PROTOC` still wins,
    // so a hand-installed protoc keeps working.
    if std::env::var_os("PROTOC").is_none() {
        // SAFETY: single-threaded build script; nothing else touches the environment.
        unsafe { std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().expect("vendored protoc binary")) };
    }

    let protowire_files = &["./proto/messages.proto", "./proto/rpc.proto"];
    let dirs = &["./proto"];

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)

        // In case we want protowire.rs to be explicitly integrated in the crate code,
        // uncomment this line and reflect the change in src/lib.rs
        //.out_dir("./src")

        .compile_protos(&protowire_files[0..1], dirs)
        .unwrap_or_else(|e| panic!("protobuf compile error: {e}"));

    // recompile protobufs only if any of the proto files changes.
    for file in protowire_files {
        println!("cargo:rerun-if-changed={file}");
    }
}
