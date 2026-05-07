use serde::de::{self, Deserializer};
use serde::Deserialize;
pub fn parse_usize_like(value: &str) -> std::result::Result<usize, String> {
    // Purpose: Parse usize from a string allowing underscore separators.
    // Inputs: raw numeric string.
    // Outputs: parsed usize or error message.
    let normalized = value.replace('_', "");
    normalized
        .parse::<usize>()
        .map_err(|e| format!("invalid usize value '{value}': {e}"))
}

pub fn deserialize_usize_flexible<'de, D>(deserializer: D) -> std::result::Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    // Purpose: Deserialize usize from YAML number or numeric string.
    // Inputs: serde deserializer.
    // Outputs: parsed usize value.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Num(u64),
        Str(String),
    }

    match Value::deserialize(deserializer)? {
        Value::Num(n) => usize::try_from(n).map_err(|_| de::Error::custom("value out of range for usize")),
        Value::Str(s) => parse_usize_like(&s).map_err(de::Error::custom),
    }
}

pub fn deserialize_option_usize_flexible<'de, D>(deserializer: D) -> std::result::Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    // Purpose: Deserialize optional usize from YAML number/string/null.
    // Inputs: serde deserializer.
    // Outputs: optional usize.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Num(u64),
        Str(String),
    }

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(Value::Num(n)) => usize::try_from(n)
            .map(Some)
            .map_err(|_| de::Error::custom("value out of range for usize")),
        Some(Value::Str(s)) => parse_usize_like(&s).map(Some).map_err(de::Error::custom),
    }
}

pub fn parse_i32_like(value: &str) -> std::result::Result<i32, String> {
    // Purpose: Parse i32 from a string allowing underscore separators.
    // Inputs: raw numeric string.
    // Outputs: parsed i32 or error message.
    let normalized = value.replace('_', "");
    normalized
        .parse::<i32>()
        .map_err(|e| format!("invalid i32 value '{value}': {e}"))
}

pub fn deserialize_option_i32_flexible<'de, D>(deserializer: D) -> std::result::Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    // Purpose: Deserialize optional i32 from YAML number/string/null.
    // Inputs: serde deserializer.
    // Outputs: optional i32.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Num(i64),
        Str(String),
    }

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(Value::Num(n)) => i32::try_from(n)
            .map(Some)
            .map_err(|_| de::Error::custom("value out of range for i32")),
        Some(Value::Str(s)) => parse_i32_like(&s).map(Some).map_err(de::Error::custom),
    }
}
