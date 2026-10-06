//! exe にアイコンを埋め込む（assets/icon.rc → assets/icon.ico）。窓の題の帯のアイコンは main.rs で別に渡す
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("assets/icon.rc", embed_resource::NONE).manifest_optional().unwrap();
}
