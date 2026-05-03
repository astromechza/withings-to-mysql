pub mod activity;
pub mod intraday;
pub mod measure;
pub mod sleep;
pub mod workouts;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Withings sometimes sends `false` instead of `0` for integer fields.
pub(crate) fn de_bool_as_i64<'de, D>(de: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrInt {
        Bool(bool),
        Int(i64),
    }
    Ok(match BoolOrInt::deserialize(de)? {
        BoolOrInt::Bool(b) => b as i64,
        BoolOrInt::Int(n) => n,
    })
}

/// Withings sometimes sends `false` instead of `null`/absent for optional integer fields.
pub(crate) fn de_bool_as_none_i64<'de, D>(de: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrIntOrNull {
        Bool(bool),
        Int(i64),
        Null,
    }
    Ok(match Option::<BoolOrIntOrNull>::deserialize(de)? {
        None | Some(BoolOrIntOrNull::Bool(false)) | Some(BoolOrIntOrNull::Null) => None,
        Some(BoolOrIntOrNull::Bool(true)) => Some(1),
        Some(BoolOrIntOrNull::Int(n)) => Some(n),
    })
}

#[derive(Debug, Deserialize)]
pub struct Envelope<B> {
    pub status: i64,
    pub body: Option<B>,
    #[serde(default)]
    pub error: Option<String>,
}

pub fn unwrap_envelope<B: for<'de> Deserialize<'de>>(json: &str) -> Result<B> {
    let env: Envelope<B> = serde_json::from_str(json).context("parse envelope")?;
    if env.status != 0 {
        bail!("withings status={} error={:?}", env.status, env.error);
    }
    env.body.context("body missing")
}
