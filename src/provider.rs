//! Decision backends. The public API is System One's; each provider translates
//! the request and the response internally.

#[cfg(feature = "clef")]
pub(crate) mod clef;
#[cfg(feature = "openai-decisions")]
pub(crate) mod openai;

use crate::constants::{API_KEY_ENV, DEFAULT_BASE_URL, DEFAULT_MODEL, PROVIDER_ENV};
use crate::error::Error;

/// The decision backend a [`Client`](crate::Client) talks to.
///
/// Every provider takes the same [`Question`](crate::Question)s and returns
/// the same [`SystemOneResponse`](crate::SystemOneResponse). A request the
/// provider cannot express fails with [`Error::Unsupported`] before anything
/// is sent. Set it with [`ClientBuilder::provider`](crate::ClientBuilder::provider),
/// or with `TYPESAFE_PROVIDER` for [`Client::from_env`](crate::Client::from_env).
///
/// | | TypeSafe | Clef | OpenAI Decisions |
/// |---|---|---|---|
/// | noul, choice, score | yes | yes | yes |
/// | box questions | yes | no | no |
/// | images | yes | no | no |
/// | structured state, instructions, criteria | yes | yes | no, strings only |
/// | noul criteria (yes/no meanings) | yes | yes | no |
/// | `models()` | yes | no | no |
/// | zstd request bodies | yes | never | never |
///
/// A self-hosted Clef server (SGLang `POST /v1/systemone`) speaks plain System
/// One: use `TypeSafe` with [`base_url`](crate::ClientBuilder::base_url).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Provider {
    /// TypeSafe System One, `POST {base}/v1/systemone`. The default.
    #[default]
    TypeSafe,
    /// Cloudflare Workers AI Clef,
    /// `POST {base}/accounts/{account_id}/ai/run/@cf/cloudflare/{model}`.
    ///
    /// The API key is a Cloudflare API token with Workers AI access. The base
    /// URL defaults to `https://api.cloudflare.com/client/v4` and the model to
    /// `clef`; `clef-flash` is the other slug. The model goes both in the path
    /// and in the body's `model`. Needs the `clef` feature.
    ///
    /// Not supported: box questions, images, `models()`, more than 64
    /// questions, question names outside `[A-Za-z0-9_.-]{1,100}`, and a choice
    /// with exactly one label, which Workers AI rejects with 422.
    #[cfg(feature = "clef")]
    Clef {
        /// Cloudflare account ID, the `{account_id}` in the URL.
        account_id: String,
    },
    /// OpenAI Decisions, `POST {base}/v1/decisions`. **Experimental**: built
    /// from the public guide, which has no API reference, and not yet checked
    /// against the live API.
    ///
    /// The base URL defaults to `https://api.openai.com` and the model to
    /// `gpt-6-luna`. Needs the `openai-decisions` feature.
    ///
    /// A noul is sent as a `predicate`, a choice's criteria as `choices`, and a
    /// score's criteria as `levels` labeled `"0"`, `"1"`, and so on. State,
    /// instructions, and criteria descriptions must be strings, and every
    /// question needs instructions. Not supported: noul criteria, box
    /// questions, images, `models()`, and raw questions that are not a plain
    /// noul, choice, or score. A refusal comes back as
    /// [`Answer::Refusal`](crate::Answer::Refusal). The response has no model
    /// name or token counts, so `model` is the requested model and `usage` is
    /// empty unless the server sends them.
    #[cfg(feature = "openai-decisions")]
    OpenAiDecisions,
}

impl Provider {
    /// The name used in errors.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::TypeSafe => "TypeSafe",
            #[cfg(feature = "clef")]
            Self::Clef { .. } => "Clef",
            #[cfg(feature = "openai-decisions")]
            Self::OpenAiDecisions => "OpenAI Decisions",
        }
    }

    /// The environment variable that holds this provider's API key.
    pub(crate) fn key_env(&self) -> &'static str {
        match self {
            Self::TypeSafe => API_KEY_ENV,
            #[cfg(feature = "clef")]
            Self::Clef { .. } => crate::constants::CLOUDFLARE_API_TOKEN_ENV,
            #[cfg(feature = "openai-decisions")]
            Self::OpenAiDecisions => crate::constants::OPENAI_API_KEY_ENV,
        }
    }

    pub(crate) fn default_base_url(&self) -> &'static str {
        match self {
            Self::TypeSafe => DEFAULT_BASE_URL,
            #[cfg(feature = "clef")]
            Self::Clef { .. } => crate::constants::CLEF_DEFAULT_BASE_URL,
            #[cfg(feature = "openai-decisions")]
            Self::OpenAiDecisions => crate::constants::OPENAI_DEFAULT_BASE_URL,
        }
    }

    pub(crate) fn default_model(&self) -> &'static str {
        match self {
            Self::TypeSafe => DEFAULT_MODEL,
            #[cfg(feature = "clef")]
            Self::Clef { .. } => crate::constants::CLEF_DEFAULT_MODEL,
            #[cfg(feature = "openai-decisions")]
            Self::OpenAiDecisions => crate::constants::OPENAI_DEFAULT_MODEL,
        }
    }

    /// Trims and checks the provider's own settings, such as the Clef account ID.
    pub(crate) fn validated(self) -> Result<Self, Error> {
        match self {
            #[cfg(feature = "clef")]
            Self::Clef { account_id } => {
                clef::validate_account_id(&account_id).map(|account_id| Self::Clef { account_id })
            }
            other => Ok(other),
        }
    }

    /// The provider `TYPESAFE_PROVIDER` names. Unset or blank is `TypeSafe`.
    pub(crate) fn from_env() -> Result<Self, Error> {
        let Some(raw) = crate::config::resolve_env(None, PROVIDER_ENV, None) else {
            return Ok(Self::TypeSafe);
        };
        match raw.to_ascii_lowercase().as_str() {
            "typesafe" => Ok(Self::TypeSafe),
            #[cfg(feature = "clef")]
            "clef" => {
                let env = crate::constants::CLOUDFLARE_ACCOUNT_ID_ENV;
                let account_id = crate::config::resolve_env(None, env, None).ok_or_else(|| {
                    Error::sdk(format!(
                        "{PROVIDER_ENV}=clef needs the Cloudflare account ID. Set the {env} environment variable."
                    ))
                })?;
                Ok(Self::Clef { account_id })
            }
            #[cfg(not(feature = "clef"))]
            "clef" => Err(feature_off("clef", "clef")),
            #[cfg(feature = "openai-decisions")]
            "openai" => Ok(Self::OpenAiDecisions),
            #[cfg(not(feature = "openai-decisions"))]
            "openai" => Err(feature_off("openai", "openai-decisions")),
            _ => Err(Error::sdk(format!(
                "Unknown {PROVIDER_ENV} value \"{raw}\". Use typesafe, clef, or openai."
            ))),
        }
    }
}

#[cfg(any(not(feature = "clef"), not(feature = "openai-decisions")))]
fn feature_off(value: &str, feature: &str) -> Error {
    Error::sdk(format!(
        "{PROVIDER_ENV}={value} needs the `{feature}` feature of typesafe-sdk."
    ))
}

/// Refuses any image use: images or aliases on the call, an `assets` key in
/// `extra_body`, or an attached image the request references.
#[cfg(any(feature = "clef", feature = "openai-decisions"))]
pub(crate) fn reject_images(
    provider: &Provider,
    json: &[u8],
    request: &crate::wire::SystemOneRequest,
    opts: &crate::client::SystemOneOpts,
    attached: &crate::asset::Attached,
) -> Result<(), Error> {
    let referenced = || {
        !crate::asset::pending_assets(
            json,
            &indexmap::IndexMap::new(),
            &[],
            crate::asset::snapshot(attached),
        )
        .is_empty()
    };
    if !opts.assets.is_empty()
        || !opts.images.is_empty()
        || request.extra.contains_key("assets")
        || referenced()
    {
        return Err(Error::unsupported(provider.name(), "images"));
    }
    Ok(())
}

/// The model the body sends: an `extra_body` `model` string wins over the
/// request's.
#[cfg(any(feature = "clef", feature = "openai-decisions"))]
pub(crate) fn effective_model(request: &crate::wire::SystemOneRequest) -> String {
    match request.extra.get("model") {
        Some(serde_json::Value::String(model)) => model.clone(),
        _ => request.model.clone().unwrap_or_default(),
    }
}

#[cfg(all(test, not(all(feature = "clef", feature = "openai-decisions"))))]
mod tests {
    use super::*;

    /// The only unit test that sets `TYPESAFE_PROVIDER`.
    #[test]
    fn a_provider_without_its_feature_names_the_feature() {
        let mut cases = Vec::new();
        if !cfg!(feature = "clef") {
            cases.push((
                "clef",
                "TYPESAFE_PROVIDER=clef needs the `clef` feature of typesafe-sdk.",
            ));
        }
        if !cfg!(feature = "openai-decisions") {
            cases.push((
                "openai",
                "TYPESAFE_PROVIDER=openai needs the `openai-decisions` feature of typesafe-sdk.",
            ));
        }
        for (value, message) in cases {
            unsafe { std::env::set_var(PROVIDER_ENV, value) };
            let error = Provider::from_env().unwrap_err();
            unsafe { std::env::remove_var(PROVIDER_ENV) };
            assert_eq!(error.to_string(), message);
        }
    }
}
