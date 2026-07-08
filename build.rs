use std::fs;
use std::io::{self, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let icon_path = Path::new(&out_dir).join("voice-watch.ico");
    write_icon(&icon_path)?;

    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(icon_path.to_string_lossy().as_ref())
        .set("InternalName", "voice-watch.exe")
        .set("OriginalFilename", "voice-watch.exe")
        .set("ProductName", "Voice Watch")
        .set("FileDescription", "Voice Watch")
        .set("CompanyName", "Voice Watch contributors")
        .set(
            "LegalCopyright",
            "Copyright (c) 2026 Voice Watch contributors",
        );
    resource.compile()?;

    Ok(())
}

fn write_icon(path: &Path) -> io::Result<()> {
    const SIZE: u32 = 32;
    let mut dib = Vec::new();

    dib.extend_from_slice(&40_u32.to_le_bytes());
    dib.extend_from_slice(&(SIZE as i32).to_le_bytes());
    dib.extend_from_slice(&((SIZE * 2) as i32).to_le_bytes());
    dib.extend_from_slice(&1_u16.to_le_bytes());
    dib.extend_from_slice(&32_u16.to_le_bytes());
    dib.extend_from_slice(&0_u32.to_le_bytes());
    dib.extend_from_slice(&(SIZE * SIZE * 4).to_le_bytes());
    dib.extend_from_slice(&0_i32.to_le_bytes());
    dib.extend_from_slice(&0_i32.to_le_bytes());
    dib.extend_from_slice(&0_u32.to_le_bytes());
    dib.extend_from_slice(&0_u32.to_le_bytes());

    for y in (0..SIZE).rev() {
        for x in 0..SIZE {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let distance = dx * dx + dy * dy;
            let (r, g, b, a) = if distance <= 14 * 14 {
                (42, 184, 120, 255)
            } else if distance <= 15 * 15 {
                (244, 247, 251, 255)
            } else {
                (0, 0, 0, 0)
            };
            dib.extend_from_slice(&[b, g, r, a]);
        }
    }

    dib.extend(std::iter::repeat_n(0_u8, (SIZE * 4) as usize));

    let image_size = dib.len() as u32;
    let image_offset = 6_u32 + 16_u32;
    let mut ico = Vec::new();
    ico.extend_from_slice(&0_u16.to_le_bytes());
    ico.extend_from_slice(&1_u16.to_le_bytes());
    ico.extend_from_slice(&1_u16.to_le_bytes());
    ico.push(SIZE as u8);
    ico.push(SIZE as u8);
    ico.push(0);
    ico.push(0);
    ico.extend_from_slice(&1_u16.to_le_bytes());
    ico.extend_from_slice(&32_u16.to_le_bytes());
    ico.extend_from_slice(&image_size.to_le_bytes());
    ico.extend_from_slice(&image_offset.to_le_bytes());
    ico.extend_from_slice(&dib);

    let mut file = fs::File::create(path)?;
    file.write_all(&ico)
}
