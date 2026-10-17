//! Send an image and locate something in it with a box question.
//!
//! Mock: `cargo run --example images --features mock`
//! Live: `TYPESAFE_LIVE=1 TYPESAFE_EXAMPLE_IMAGE=photo.png cargo run --example images`
//!
//! The live API rejects very small images, so pass a real photo in
//! `TYPESAFE_EXAMPLE_IMAGE` when running live.

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/harness.rs"]
mod harness;

use typesafe_sdk::{Image, Question, SystemOneOpts};

/// A 1x1 PNG, enough for the mock.
const PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[tokio::main]
async fn main() -> Result<(), typesafe_sdk::Error> {
    let rt = harness::ExampleRuntime::start(fixtures::image_box()).await?;
    println!("mode={}", harness::ExampleRuntime::mode_label());

    let photo = match std::env::var_os("TYPESAFE_EXAMPLE_IMAGE") {
        Some(path) => Image::from_path(path),
        None => Image::from_bytes(PIXEL_PNG),
    };
    // Image support is opt-in on the API. A client that always sends images can
    // set this once with `ClientBuilder::header`.
    let mut extra_headers = http::HeaderMap::new();
    extra_headers.insert(
        "typesafe-danger-zone-images",
        http::HeaderValue::from_static("true"),
    );

    let response = rt
        .client
        .system_one_opts(
            // `{photo}` writes the image's reference into the text.
            format!("Listing photo: {photo}"),
            [
                ("product", Question::box_("The product for sale")),
                (
                    "damaged",
                    Question::noul("Does the product in the photo look damaged?"),
                ),
            ],
            SystemOneOpts {
                images: vec![photo.clone()],
                extra_headers,
                ..SystemOneOpts::default()
            },
        )
        .await?;

    let product = response.box_("product")?;
    println!(
        "product top={} bottom={} left={} right={} confidence={:?}",
        product.box_.top,
        product.box_.bottom,
        product.box_.left,
        product.box_.right,
        product.confidence
    );
    println!("damaged={}", response.noul("damaged")?.noul);
    Ok(())
}
