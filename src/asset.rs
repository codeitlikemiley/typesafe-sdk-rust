//! Image assets sent with a System One request.

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine;
use bytes::Bytes;
use indexmap::IndexMap;

use crate::error::Error;

/// Content type of a PNG file, recognized by its signature.
const PNG: (&[u8], &str) = (b"\x89PNG\r\n\x1a\n", "image/png");
/// Content type of a JPEG file, recognized by its signature.
const JPEG: (&[u8], &str) = (b"\xff\xd8\xff", "image/jpeg");
/// Content type of any other image. The API detects formats itself.
const UNKNOWN_CONTENT_TYPE: &str = "application/octet-stream";
/// Literal start of every image reference.
const REFERENCE_PREFIX: &[u8] = b"<image:";
/// Number of lowercase hex digits in a reference.
const REFERENCE_HEX_LEN: usize = 32;

/// An image sent with a System One request, from bytes in memory or a file.
///
/// Each `Image` has a unique reference, `<image:` followed by 32 lowercase hex
/// digits and `>`. [`Display`](fmt::Display) and [`Image::reference`] return it,
/// so `format!("Compare {image}")` puts the image in text. The reference may go
/// in `state`, question instructions, or criteria, as or inside any string. The
/// client sends an image when its reference appears in the request and the image
/// is known to the call: passed in
/// [`SystemOneOpts::images`](crate::SystemOneOpts::images) or
/// [`SystemOneOpts::assets`](crate::SystemOneOpts::assets), or attached with
/// [`Client::attach_assets`](crate::Client::attach_assets). `assets` also
/// takes named placeholders, such as `<photo>`.
///
/// Cloning is cheap and keeps the identity: a clone is the same asset. Two
/// `Image`s made from the same bytes are different assets with different
/// references. Equality and hashing follow identity. Clippy's
/// `mutable_key_type` lint flags an `Image` key because of the read-once cache
/// inside; hashing does not depend on it.
///
/// The data is read and checked when a request first sends the image, then kept
/// for every later request. A path is read then, not when the `Image` is made,
/// and later changes to the file are not seen. Only the file signature is
/// inspected: PNG is sent as `image/png`, JPEG as `image/jpeg`, and anything
/// else as `application/octet-stream`. The SDK sets no size limit; the API
/// validates image contents.
///
/// Image requests need a vision-language model. The API currently also needs
/// the opt-in header `TypeSafe-Danger-Zone-Images: true`, which the client
/// does not add; set it with [`ClientBuilder::header`](crate::ClientBuilder::header).
#[derive(Clone)]
pub struct Image {
    inner: Arc<Inner>,
}

struct Inner {
    reference: String,
    source: Source,
    resolved: OnceLock<Resolved>,
}

enum Source {
    Bytes(Bytes),
    Path(PathBuf),
}

/// An image with its data read and checked.
#[derive(Clone)]
pub(crate) struct Resolved {
    pub content_type: &'static str,
    pub data: Bytes,
}

impl Image {
    /// Makes an image from image file bytes, such as the contents of a PNG file.
    ///
    /// Empty data is an error when a request sends the image.
    pub fn from_bytes(data: impl Into<Bytes>) -> Self {
        Self::new(Source::Bytes(data.into()))
    }

    /// Makes an image from a path to an image file.
    ///
    /// The file is read when a request first sends the image, and the bytes are
    /// kept after that. A file that cannot be read is an error at that request,
    /// such as `Asset '<photo>': Image file 'missing.png' could not be read: No
    /// such file or directory.`
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self::new(Source::Path(path.into()))
    }

    /// Makes an image from base64 text of image file bytes, such as an upload.
    ///
    /// Whitespace, such as the line breaks of wrapped base64, is ignored. The
    /// text must use the standard alphabet with padding. A `data:` URL prefix is
    /// not accepted. Decodes now and errors with `Image data must contain
    /// non-empty ASCII base64 text.` or `Image data must contain valid base64
    /// text.`
    pub fn from_base64(data: &str) -> Result<Self, Error> {
        let text: String = if data.is_ascii() {
            data.chars().filter(|c| !is_python_space(*c)).collect()
        } else {
            String::new()
        };
        if text.is_empty() {
            return Err(Error::sdk(
                "Image data must contain non-empty ASCII base64 text.",
            ));
        }
        let engine = base64::engine::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::GeneralPurposeConfig::new()
                .with_decode_allow_trailing_bits(true)
                .with_decode_padding_mode(base64::engine::DecodePaddingMode::RequireCanonical),
        );
        let bytes = engine
            .decode(text.as_bytes())
            .map_err(|_| Error::sdk("Image data must contain valid base64 text."))?;
        Ok(Self::from_bytes(bytes))
    }

    /// Returns the reference that stands for this image in text,
    /// `<image:{32 lowercase hex digits}>`. The same as `to_string()`.
    pub fn reference(&self) -> &str {
        &self.inner.reference
    }

    fn new(source: Source) -> Self {
        Self {
            inner: Arc::new(Inner {
                reference: format!("<image:{:032x}>", fastrand::u128(..)),
                source,
                resolved: OnceLock::new(),
            }),
        }
    }

    /// Reads and checks the image once, then returns the kept result.
    pub(crate) async fn resolve(&self) -> Result<Resolved, Error> {
        if let Some(resolved) = self.inner.resolved.get() {
            return Ok(resolved.clone());
        }
        let data = match &self.inner.source {
            Source::Bytes(bytes) => bytes.clone(),
            Source::Path(path) => match tokio::fs::read(path).await {
                Ok(bytes) => Bytes::from(bytes),
                Err(error) => {
                    return Err(Error::sdk(format!(
                        "Image file {} could not be read: {}.",
                        py_repr(&path.to_string_lossy()),
                        strerror(&error)
                    )));
                }
            },
        };
        if data.is_empty() {
            return Err(Error::sdk("Image data is empty."));
        }
        let resolved = Resolved {
            content_type: content_type(&data),
            data,
        };
        Ok(self.inner.resolved.get_or_init(|| resolved).clone())
    }
}

impl fmt::Display for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.reference)
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("Image");
        debug.field("reference", &self.inner.reference);
        match &self.inner.source {
            Source::Bytes(bytes) => debug.field("bytes", &bytes.len()),
            Source::Path(path) => debug.field("path", path),
        };
        debug.finish()
    }
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for Image {}

impl Hash for Image {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.inner).hash(state);
    }
}

/// Images attached to a client, keyed by reference, with a hold count each.
pub(crate) type Attached = Arc<Mutex<HashMap<String, (Image, usize)>>>;

/// Keeps images attached to a [`Client`](crate::Client) until it is dropped.
///
/// Returned by [`Client::attach_assets`](crate::Client::attach_assets). While
/// the guard lives, every call on that client, and on its clones, sends an
/// attached image whose reference appears in the request. Attaching the same
/// image again holds it again; it stays attached until the last guard holding
/// it is dropped. The guard is `Send`, so it can move into a spawned task.
#[must_use = "the images are released when the guard is dropped"]
pub struct AttachGuard {
    attached: Attached,
    images: Vec<Image>,
}

impl AttachGuard {
    pub(crate) fn new(attached: &Attached, images: impl IntoIterator<Item = Image>) -> Self {
        let images: Vec<Image> = images.into_iter().collect();
        let mut map = lock(attached);
        for image in &images {
            map.entry(image.reference().to_string())
                .or_insert_with(|| (image.clone(), 0))
                .1 += 1;
        }
        drop(map);
        Self {
            attached: Arc::clone(attached),
            images,
        }
    }
}

impl Drop for AttachGuard {
    fn drop(&mut self) {
        let mut map = lock(&self.attached);
        for image in &self.images {
            if let Some((_, holds)) = map.get_mut(image.reference()) {
                *holds -= 1;
                if *holds == 0 {
                    map.remove(image.reference());
                }
            }
        }
    }
}

impl fmt::Debug for AttachGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachGuard")
            .field("images", &self.images)
            .finish()
    }
}

/// The currently attached images, keyed by reference.
pub(crate) fn snapshot(attached: &Attached) -> HashMap<String, Image> {
    lock(attached)
        .iter()
        .map(|(reference, (image, _))| (reference.clone(), image.clone()))
        .collect()
}

fn lock(attached: &Attached) -> std::sync::MutexGuard<'_, HashMap<String, (Image, usize)>> {
    attached
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The images a request sends, in `assets` order: every alias in `assets`, then
/// each reference that appears in `body` and names a known image, in order of
/// first appearance. `known` is the attached images, the passed images, and the
/// alias values.
pub(crate) fn pending_assets(
    body: &[u8],
    assets: &IndexMap<String, Image>,
    images: &[Image],
    attached: HashMap<String, Image>,
) -> IndexMap<String, Image> {
    let mut pending = assets.clone();
    let mut known = attached;
    for image in images.iter().chain(assets.values()) {
        known.insert(image.reference().to_string(), image.clone());
    }
    if known.is_empty() {
        return pending;
    }
    for reference in references(body) {
        if let Some(image) = known.get(reference) {
            pending
                .entry(reference.to_string())
                .or_insert_with(|| image.clone());
        }
    }
    pending
}

/// Every `<image:{32 lowercase hex}>` in `body`, in order, without overlaps.
fn references(body: &[u8]) -> impl Iterator<Item = &str> {
    let total = REFERENCE_PREFIX.len() + REFERENCE_HEX_LEN + 1;
    let mut at = 0;
    std::iter::from_fn(move || {
        while at + total <= body.len() {
            let candidate = &body[at..at + total];
            let hex = &candidate[REFERENCE_PREFIX.len()..total - 1];
            if candidate.starts_with(REFERENCE_PREFIX)
                && candidate[total - 1] == b'>'
                && hex.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            {
                at += total;
                // The bytes are ASCII, so this cannot fail.
                return std::str::from_utf8(candidate).ok();
            }
            at += 1;
        }
        None
    })
}

/// Checks every alias, then reads each distinct image once. Errors name the
/// first failing alias in order, such as `Asset '<photo>': Image data is empty.`
pub(crate) async fn resolve_assets(
    pending: &IndexMap<String, Image>,
) -> Result<Vec<Resolved>, Error> {
    if pending.keys().any(|alias| alias.trim().is_empty()) {
        return Err(Error::sdk("Asset aliases must be non-empty strings."));
    }
    // Keyed by reference, which is unique to each image.
    let mut done: HashMap<&str, Resolved> = HashMap::new();
    let mut out = Vec::with_capacity(pending.len());
    for (alias, image) in pending {
        let resolved = match done.get(image.reference()) {
            Some(resolved) => resolved.clone(),
            None => {
                let resolved = image
                    .resolve()
                    .await
                    .map_err(|error| Error::sdk(format!("Asset {}: {error}", py_repr(alias))))?;
                done.insert(image.reference(), resolved.clone());
                resolved
            }
        };
        out.push(resolved);
    }
    Ok(out)
}

fn content_type(data: &[u8]) -> &'static str {
    for (signature, content_type) in [PNG, JPEG] {
        if data.starts_with(signature) {
            return content_type;
        }
    }
    UNKNOWN_CONTENT_TYPE
}

/// Whether Python's `str.split()` treats an ASCII character as whitespace.
fn is_python_space(c: char) -> bool {
    matches!(c, ' ' | '\t'..='\r' | '\x1c'..='\x1f')
}

/// The OS error text without Rust's ` (os error N)` suffix, as Python's `strerror`.
fn strerror(error: &std::io::Error) -> String {
    let text = error.to_string();
    match error.raw_os_error() {
        Some(code) => text
            .strip_suffix(&format!(" (os error {code})"))
            .unwrap_or(&text)
            .to_string(),
        None => text,
    }
}

/// Python's `repr` of a string, which the Python SDK's messages use.
pub(crate) fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
