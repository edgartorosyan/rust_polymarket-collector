use serde::{Deserialize, Deserializer};

/// Deserializes an `Option<f64>` from either a JSON number or a quoted string.
/// Handles the Gamma API inconsistency where some fields come as `"5602.49"` (string)
/// at the nested market level but as `5602.49` (number) at the event level.
pub fn deserialize_opt_f64_or_str<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(f64),
        Str(String),
    }

    match Option::<NumOrStr>::deserialize(deserializer)? {
        None => Ok(None),
        Some(NumOrStr::Num(n)) => Ok(Some(n)),
        Some(NumOrStr::Str(s)) => s
            .parse::<f64>()
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}
