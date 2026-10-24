#[path = "src/icon_gen.rs"]
mod icon_gen;

/// Schreibt eine .ico-Datei (BMP-Einträge) mit mehreren Größen.
fn write_ico(path: &std::path::Path) {
    let sizes = [16usize, 32, 48, 64, 128, 256];
    let mut images = Vec::new();
    for &s in &sizes {
        let rgba = icon_gen::icon_rgba(s);
        let mut bmp = Vec::new();
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(s as i32).to_le_bytes());
        bmp.extend_from_slice(&((s * 2) as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&32u16.to_le_bytes());
        bmp.extend_from_slice(&[0u8; 24]);
        for y in (0..s).rev() {
            for x in 0..s {
                let o = (y * s + x) * 4;
                bmp.extend_from_slice(&[rgba[o + 2], rgba[o + 1], rgba[o], rgba[o + 3]]);
            }
        }
        let mask_row = s.div_ceil(32) * 4;
        bmp.extend(std::iter::repeat_n(0u8, mask_row * s));
        images.push((s, bmp));
    }
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (s, data) in &images {
        let b = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[b, b, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in &images {
        ico.extend_from_slice(data);
    }
    std::fs::write(path, ico).expect("Icon schreiben");
}

fn main() {
    println!("cargo:rerun-if-changed=src/icon_gen.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app.ico");
        write_ico(&out);
        let mut res = winresource::WindowsResource::new();
        res.set_icon(out.to_str().unwrap());
        res.set("ProductName", "Skin Studio");
        res.set("FileDescription", "Skin Studio - Minecraft Skin Editor");
        res.set("LegalCopyright", "Freie Nutzung");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Icon konnte nicht eingebettet werden: {e}");
        }
    }
}
