//! Compiles `proto/modbit/v1/*.proto` into Rust with a pure-Rust protobuf
//! compiler, so a fresh clone needs no system `protoc` (docs/70
//! reproducibility). Generated code is written to `OUT_DIR`; the schema source
//! under `proto/` is the only authority.

use std::path::PathBuf;

fn main() {
    let proto_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("proto");
    let mut files: Vec<PathBuf> = std::fs::read_dir(proto_root.join("modbit/v1"))
        .expect("proto/modbit/v1 exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "proto"))
        .collect();
    files.sort();
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    let descriptors = protox::compile(&files, [&proto_root]).expect("protobuf sources compile");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(
        out.join("modbit_v1_descriptor.bin"),
        prost::Message::encode_to_vec(&descriptors),
    )
    .expect("write descriptor set");
    prost_build::Config::new()
        .compile_fds(descriptors)
        .expect("prost code generation");
}
