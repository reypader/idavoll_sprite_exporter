use anyhow::{anyhow, Result};
use std::io::{Cursor, Read};

#[derive(Debug, Clone, Copy, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// 256-entry RGBA palette
pub type Palette = [Color; 256];

#[derive(Debug, Clone)]
pub struct RawImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<Color>, // row-major, top-to-bottom, RGBA
}

pub struct SprFile {
    pub version: u16,
    #[allow(dead_code)]
    pub palette: Palette,
    pub palette_images: Vec<RawImage>,
    pub rgba_images: Vec<RawImage>,
}

impl SprFile {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 2 + 2 + 2 + 1024 {
            return Err(anyhow!("SPR file too small"));
        }

        let mut c = Cursor::new(data);

        let mut sig = [0u8; 2];
        c.read_exact(&mut sig)?;
        if &sig != b"SP" {
            return Err(anyhow!("Invalid SPR signature"));
        }

        let version = ru16(&mut c)?;
        let pal_count = ru16(&mut c)? as usize;
        let rgba_count = if version >= 0x200 {
            ru16(&mut c)? as usize
        } else {
            0
        };

        // Palette is always the last 1024 bytes of the file
        let palette = read_palette(&data[data.len() - 1024..]);

        // --- Palette-indexed images ---
        let mut palette_images = Vec::with_capacity(pal_count);
        for _ in 0..pal_count {
            let width = ru16(&mut c)?;
            let height = ru16(&mut c)?;
            let pixel_count = width as usize * height as usize;

            let pixels = if version >= 0x201 {
                // RLE compressed: 2-byte compressed size then data
                let compressed_size = ru16(&mut c)? as usize;
                let mut compressed = vec![0u8; compressed_size];
                c.read_exact(&mut compressed)?;
                decode_rle(&compressed, pixel_count, &palette)
            } else {
                // Raw palette indices
                let mut indices = vec![0u8; pixel_count];
                c.read_exact(&mut indices)?;
                indices
                    .iter()
                    .map(|&i| {
                        let mut col = palette[i as usize];
                        col.a = if i == 0 { 0 } else { 255 };
                        col
                    })
                    .collect()
            };

            palette_images.push(RawImage {
                width,
                height,
                pixels,
            });
        }

        // --- RGBA truecolor images ---
        // Stored as ABGR in file, with Y-axis inverted
        let mut rgba_images = Vec::with_capacity(rgba_count);
        for _ in 0..rgba_count {
            let width = ru16(&mut c)?;
            let height = ru16(&mut c)?;
            let w = width as usize;
            let h = height as usize;
            let mut pixels = vec![Color::default(); w * h];

            for src_row in 0..h {
                let dest_row = h - 1 - src_row; // flip Y
                for col in 0..w {
                    let mut abgr = [0u8; 4];
                    c.read_exact(&mut abgr)?;
                    pixels[dest_row * w + col] = Color {
                        r: abgr[3],
                        g: abgr[2],
                        b: abgr[1],
                        a: abgr[0],
                    };
                }
            }

            rgba_images.push(RawImage {
                width,
                height,
                pixels,
            });
        }

        Ok(SprFile {
            version,
            palette,
            palette_images,
            rgba_images,
        })
    }

    pub fn get_image(&self, spr_id: i32, spr_type: i32) -> Option<&RawImage> {
        if spr_id < 0 {
            return None;
        }
        match spr_type {
            0 => self.palette_images.get(spr_id as usize),
            1 => self.rgba_images.get(spr_id as usize),
            _ => None,
        }
    }
}

fn decode_rle(compressed: &[u8], expected: usize, palette: &Palette) -> Vec<Color> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < compressed.len() && out.len() < expected {
        let idx = compressed[i];
        i += 1;
        if idx == 0 {
            // RLE run: next byte is count of transparent pixels
            let count = if i < compressed.len() {
                compressed[i] as usize
            } else {
                0
            };
            i += 1;
            for _ in 0..count {
                out.push(Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                });
            }
        } else {
            let mut col = palette[idx as usize];
            col.a = 255;
            out.push(col);
        }
    }
    // Pad to expected size with transparent pixels
    out.resize(expected, Color::default());
    out
}

fn read_palette(data: &[u8]) -> Palette {
    let mut palette = [Color::default(); 256];
    for (i, entry) in palette.iter_mut().enumerate() {
        let base = i * 4;
        *entry = Color {
            r: data[base],
            g: data[base + 1],
            b: data[base + 2],
            a: 255, // palette alpha is ignored; index 0 is transparent
        };
    }
    palette[0].a = 0;
    palette
}

fn ru16(c: &mut Cursor<&[u8]>) -> Result<u16> {
    let mut b = [0u8; 2];
    c.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}
