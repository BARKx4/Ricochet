use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum RicochetResult {
    Ok(Box<Value>),
    Err(RicochetError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RicochetError {
    pub kind: String,
    pub message: String,
}
