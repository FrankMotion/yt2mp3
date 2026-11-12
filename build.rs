// Embeds the icon and the Windows manifest (modern controls, DPI awareness).
fn main() {
    println!("cargo:rerun-if-changed=res/app.rc");
    println!("cargo:rerun-if-changed=res/app.manifest");
    println!("cargo:rerun-if-changed=res/icon.ico");
    let _ = embed_resource::compile("res/app.rc", embed_resource::NONE);
}
