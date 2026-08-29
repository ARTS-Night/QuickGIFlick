use std::{env, fs, path::PathBuf};

fn main() {
    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let png = include_bytes!("assets/brand/logo.png");
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    ico.extend_from_slice(&[32, 32, 0, 0, 1, 0, 32, 0]);
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(png);

    let icon_path =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("quickgiflick.ico");
    fs::write(&icon_path, ico).expect("write generated icon");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon_path.to_str().expect("icon path is UTF-8"));
    resource.compile().expect("compile Windows icon resource");
}
