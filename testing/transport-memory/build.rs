#[path = "../../utils/tower/build_transport.rs"]
mod build_transport;
fn main() {
    if std::env::var_os("PROTOC").is_none() {
        unsafe { std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap()) };
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for (name, dir, package, req, res) in [
        ("p2p", "../../protocol/p2p/proto", "protowire", "KaspadMessage", "KaspadMessage"),
        ("rpc", "../../rpc/grpc/core/proto", "protowire.kaspapq", "KaspadRequest", "KaspadResponse"),
    ] {
        let destination = out.join(name);
        std::fs::create_dir_all(&destination).unwrap();
        tonic_prost_build::configure()
            .out_dir(&destination)
            .compile_protos(&[format!("{dir}/messages.proto")], &[dir.to_owned()])
            .unwrap();
        build_transport::budget_generated_transport(&destination.join(format!("{package}.rs")), req, res);
        println!("cargo:rerun-if-changed={dir}");
    }
    tonic_prost_build::configure()
        .build_client(false)
        .build_server(false)
        .out_dir(&out)
        .compile_protos(&["fixtures/memory.proto"], &["fixtures"])
        .unwrap();
    build_transport::budget_generated_messages(&out.join("oom.memory.rs"));
    println!("cargo:rerun-if-changed=fixtures/memory.proto");
    println!("cargo:rerun-if-changed=../../utils/tower/build_transport.rs");
    println!("cargo:rerun-if-changed=../../utils/tower/build_wire_preflight.rs");
}
