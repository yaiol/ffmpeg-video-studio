//! The size of a picture file, read from its header (PNG, JPEG, GIF, BMP, WebP): what the work view needs to draw a watermark's box on the video at
//! the proportions ffmpeg will give it, without decoding anything. A file that is not one of these, or cannot be read, has no size here and the
//! box is drawn square.

use std::{fs::File, io::Read, path::Path};

/// Width and height in pixels, when the file is a picture this reads.
pub fn size(path: &Path) -> Option<(u32, u32)> {
    // a JPEG's size sits after its tables, so a file is read up to a few hundred kilobytes before giving up
    let mut head = Vec::new();
    File::open(path).ok()?.take(512 * 1024).read_to_end(&mut head).ok()?;
    of(&head).filter(|(w, h)| *w > 0 && *h > 0)
}

fn of(b: &[u8]) -> Option<(u32, u32)> {
    let be32 = |at: usize| b.get(at..at + 4).map(|v| u32::from_be_bytes([v[0], v[1], v[2], v[3]]));
    let le16 = |at: usize| b.get(at..at + 2).map(|v| u32::from(u16::from_le_bytes([v[0], v[1]])));
    let le32 = |at: usize| b.get(at..at + 4).map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?));
    }
    if b.starts_with(b"\xff\xd8") {
        return crate::preview::jpeg_size(b);
    }
    if b.starts_with(b"GIF8") {
        return Some((le16(6)?, le16(8)?));
    }
    if b.starts_with(b"BM") {
        // (a negative height is a bottom-up flag, not a size)
        return Some((le32(18)?, (le32(22)? as i32).unsigned_abs()));
    }
    if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        return match b.get(12..16)? {
            // (the extended form holds each size less one, in 24 bits)
            b"VP8X" => Some(((le32(24)? & 0x00ff_ffff) + 1, (le32(27)? & 0x00ff_ffff) + 1)),
            b"VP8L" => {
                let bits = le32(21)?;
                Some(((bits & 0x3fff) + 1, (bits >> 14 & 0x3fff) + 1))
            }
            b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_gif_and_bmp_header_say_their_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend([0, 0, 1, 64, 0, 0, 0, 200]);
        assert_eq!(of(&png), Some((320, 200)));
        let mut gif = b"GIF89a".to_vec();
        gif.extend([64, 0, 32, 0]);
        assert_eq!(of(&gif), Some((64, 32)));
        let mut bmp = vec![0_u8; 26];
        bmp[0] = b'B';
        bmp[1] = b'M';
        bmp[18..22].copy_from_slice(&50_u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&(-30_i32).to_le_bytes());
        assert_eq!(of(&bmp), Some((50, 30)));
    }

    #[test]
    fn a_webp_lossless_header_says_its_size() {
        // 'RIFF' size 'WEBP' 'VP8L' chunk-size signature(0x2f) then 14 bits of width - 1 and 14 bits of height - 1
        let bits: u32 = 99 | (49 << 14);
        let mut webp = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        webp.extend(bits.to_le_bytes());
        assert_eq!(of(&webp), Some((100, 50)));
    }

    #[test]
    fn something_else_has_no_size() {
        assert_eq!(of(b"not a picture at all"), None);
        assert_eq!(of(b""), None);
        assert_eq!(size(Path::new("C:/nowhere/none.png")), None);
    }
}
