use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::List(_) => "list",
            Value::Map(_) => "map",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SerializerError {
    #[error("expected {expected}, found {found}")]
    TypeMismatch {
        expected: &'static str,
        found: &'static str,
    },
    #[error("missing field `{0}`")]
    MissingField(String),
    #[error("format error: {0}")]
    Format(String),
}

impl SerializerError {
    pub fn code(&self) -> u16 {
        match self {
            SerializerError::TypeMismatch { .. } => 1,
            SerializerError::MissingField(_) => 2,
            SerializerError::Format(_) => 3,
        }
    }
}

pub trait Format {
    fn encode(&self, value: &Value) -> Result<Vec<u8>, SerializerError>;
    fn decode(&self, bytes: &[u8]) -> Result<Value, SerializerError>;
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::Int(i64::from(value))
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Int(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_owned())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(value: Vec<T>) -> Self {
        Value::List(value.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Value::Null, Into::into)
    }
}

impl<T: Into<Value>> From<BTreeMap<String, T>> for Value {
    fn from(value: BTreeMap<String, T>) -> Self {
        Value::Map(value.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

fn mismatch(expected: &'static str, found: &Value) -> SerializerError {
    SerializerError::TypeMismatch {
        expected,
        found: found.kind(),
    }
}

impl TryFrom<Value> for bool {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Bool(v) => Ok(v),
            other => Err(mismatch("bool", &other)),
        }
    }
}

impl TryFrom<Value> for i64 {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Int(v) => Ok(v),
            other => Err(mismatch("int", &other)),
        }
    }
}

impl TryFrom<Value> for i32 {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let wide = i64::try_from(value)?;
        i32::try_from(wide).map_err(|e| SerializerError::Format(e.to_string()))
    }
}

impl TryFrom<Value> for f64 {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Float(v) => Ok(v),
            Value::Int(v) => Ok(v as f64),
            other => Err(mismatch("float", &other)),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(v) => Ok(v),
            other => Err(mismatch("string", &other)),
        }
    }
}

impl<T> TryFrom<Value> for Vec<T>
where
    T: TryFrom<Value, Error = SerializerError>,
{
    type Error = SerializerError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::List(items) => items.into_iter().map(T::try_from).collect(),
            other => Err(mismatch("list", &other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_value() {
        assert_eq!(bool::try_from(Value::from(true)), Ok(true));
        assert_eq!(i64::try_from(Value::from(7_i64)), Ok(7));
        assert_eq!(String::try_from(Value::from("a")), Ok("a".to_owned()));
        assert_eq!(
            Vec::<i32>::try_from(Value::from(vec![1, 2])),
            Ok(vec![1, 2])
        );
    }

    #[test]
    fn rejects_a_mismatched_type() {
        let err = bool::try_from(Value::Int(1)).unwrap_err();
        assert_eq!(
            err,
            SerializerError::TypeMismatch {
                expected: "bool",
                found: "int"
            }
        );
    }

    #[test]
    fn error_codes_are_unique() {
        let codes = [
            SerializerError::TypeMismatch {
                expected: "a",
                found: "b",
            }
            .code(),
            SerializerError::MissingField(String::new()).code(),
            SerializerError::Format(String::new()).code(),
        ];
        let mut sorted = codes;
        sorted.sort_unstable();
        assert!(sorted.windows(2).all(|w| w[0] != w[1]));
    }
}
