//! Collage d'une image (ou d'un texte) depuis le champ texte du téléphone.
//!
//! Le téléphone envoie le contenu brut par `POST /paste` ;
//! ce module le valide et le décode, indépendamment du système. L'écriture dans
//! le presse-papiers passe par le port [`ClipboardSink`].

/// Limite d'un envoi : une capture d'écran d'iPhone en PNG pèse quelques Mo.
pub const MAX_BODY_BYTES: usize = 40 * 1024 * 1024;
/// Limite d'un texte, en octets UTF-8.
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
/// Côté maximal d'une image, en pixels.
pub const MAX_IMAGE_SIDE: u32 = 16_384;
/// Mémoire maximale pour décoder une image (protège contre les « bombes » de décompression).
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// Image décodée, 4 octets par pixel (rouge, vert, bleu, alpha), ligne par ligne depuis le haut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardContent {
    Text(String),
    Image(RgbaImage),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    #[error("le presse-papiers est vide")]
    Empty,
    #[error("contenu trop volumineux")]
    TooLarge,
    #[error("type de contenu non pris en charge : {0}")]
    Unsupported(String),
    #[error("le texte n'est pas en UTF-8")]
    BadText,
    #[error("image illisible : {0}")]
    BadImage(String),
}

/// Port vers le presse-papiers du système.
pub trait ClipboardSink: Send + Sync {
    fn set(&self, content: &ClipboardContent) -> Result<(), String>;
}

/// Hors Windows : se contente de journaliser.
pub struct LogClipboard;

impl ClipboardSink for LogClipboard {
    fn set(&self, content: &ClipboardContent) -> Result<(), String> {
        match content {
            ClipboardContent::Text(t) => {
                tracing::info!(chars = t.chars().count(), "presse-papiers : texte")
            }
            ClipboardContent::Image(i) => {
                tracing::info!(width = i.width, height = i.height, "presse-papiers : image")
            }
        }
        Ok(())
    }
}

/// Décode le corps d'un `POST /clipboard` selon son `Content-Type`.
pub fn decode(content_type: &str, body: &[u8]) -> Result<ClipboardContent, ClipboardError> {
    if body.is_empty() {
        return Err(ClipboardError::Empty);
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(ClipboardError::TooLarge);
    }
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if mime.starts_with("text/") {
        if body.len() > MAX_TEXT_BYTES {
            return Err(ClipboardError::TooLarge);
        }
        let text = std::str::from_utf8(body).map_err(|_| ClipboardError::BadText)?;
        // Windows attend des fins de ligne CRLF dans le presse-papiers.
        return Ok(ClipboardContent::Text(to_crlf(text)));
    }
    let format = match mime.as_str() {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" | "image/jpg" => image::ImageFormat::Jpeg,
        "image/gif" => image::ImageFormat::Gif,
        "image/webp" => image::ImageFormat::WebP,
        "image/bmp" => image::ImageFormat::Bmp,
        _ => return Err(ClipboardError::Unsupported(mime)),
    };
    decode_image(format, body).map(ClipboardContent::Image)
}

fn decode_image(format: image::ImageFormat, body: &[u8]) -> Result<RgbaImage, ClipboardError> {
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(body), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|e| match e {
            image::ImageError::Limits(_) => ClipboardError::TooLarge,
            other => ClipboardError::BadImage(other.to_string()),
        })?
        .into_rgba8();
    Ok(RgbaImage {
        width: img.width(),
        height: img.height(),
        pixels: img.into_raw(),
    })
}

fn to_crlf(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 32);
    let mut prev_cr = false;
    for c in text.chars() {
        if c == '\n' && !prev_cr {
            out.push('\r');
        }
        prev_cr = c == '\r';
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba(rgba));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn text_is_decoded_as_utf8_with_windows_line_endings() {
        let c = decode(
            "text/plain;charset=utf-8",
            "Café\nligne 2\r\nfin".as_bytes(),
        )
        .unwrap();
        assert_eq!(c, ClipboardContent::Text("Café\r\nligne 2\r\nfin".into()));
    }

    #[test]
    fn invalid_utf8_text_is_refused() {
        assert_eq!(
            decode("text/plain", &[0xff, 0xfe, 0x00]),
            Err(ClipboardError::BadText)
        );
    }

    #[test]
    fn empty_body_is_refused() {
        assert_eq!(decode("text/plain", b""), Err(ClipboardError::Empty));
    }

    #[test]
    fn png_is_decoded_to_rgba_and_keeps_transparency() {
        let c = decode("image/png", &png(3, 2, [10, 20, 30, 128])).unwrap();
        let ClipboardContent::Image(img) = c else {
            panic!("image attendue")
        };
        assert_eq!((img.width, img.height), (3, 2));
        assert_eq!(img.pixels.len(), 3 * 2 * 4);
        assert_eq!(&img.pixels[..4], &[10, 20, 30, 128]);
    }

    #[test]
    fn jpeg_is_decoded_too() {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 100, 50]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        let c = decode("IMAGE/JPEG", &out.into_inner()).unwrap();
        assert!(matches!(
            c,
            ClipboardContent::Image(RgbaImage {
                width: 4,
                height: 4,
                ..
            })
        ));
    }

    #[test]
    fn corrupt_image_is_refused() {
        assert!(matches!(
            decode("image/png", b"pas une image"),
            Err(ClipboardError::BadImage(_))
        ));
    }

    #[test]
    fn unknown_types_are_refused() {
        assert_eq!(
            decode("application/octet-stream", b"x"),
            Err(ClipboardError::Unsupported(
                "application/octet-stream".into()
            ))
        );
    }

    #[test]
    fn oversized_images_are_refused_before_allocating() {
        let bytes = png(MAX_IMAGE_SIDE + 1, 1, [0, 0, 0, 0]);
        assert_eq!(decode("image/png", &bytes), Err(ClipboardError::TooLarge));
    }

    #[test]
    fn oversized_text_is_refused() {
        let big = vec![b'a'; MAX_TEXT_BYTES + 1];
        assert_eq!(decode("text/plain", &big), Err(ClipboardError::TooLarge));
    }
}
