//! `multipart/form-data` request bodies (RFC 7578), byte for byte as the Python SDK writes them.

/// One body part. `name` and `filename` are written unescaped, so they must be
/// ASCII without quotes.
pub(crate) struct Part<'a> {
    pub name: &'a str,
    pub content_type: &'a str,
    pub content: &'a [u8],
    pub filename: Option<&'a str>,
}

/// Encodes `parts` with a random 32-hex-digit boundary. Returns the
/// `Content-Type` header value and the body.
pub(crate) fn encode(parts: &[Part<'_>]) -> (String, Vec<u8>) {
    let boundary = format!("{:032x}", fastrand::u128(..));
    let size = parts
        .iter()
        .map(|part| part.content.len() + 128)
        .sum::<usize>();
    let mut body = Vec::with_capacity(size);
    for part in parts {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
        body.extend_from_slice(part.name.as_bytes());
        body.push(b'"');
        if let Some(filename) = part.filename {
            body.extend_from_slice(b"; filename=\"");
            body.extend_from_slice(filename.as_bytes());
            body.push(b'"');
        }
        body.extend_from_slice(b"\r\nContent-Type: ");
        body.extend_from_slice(part.content_type.as_bytes());
        body.extend_from_slice(b"\r\n\r\n");
        body.extend_from_slice(part.content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    (format!("multipart/form-data; boundary={boundary}"), body)
}
