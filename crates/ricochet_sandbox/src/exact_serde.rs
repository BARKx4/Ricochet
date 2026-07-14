use serde::{Deserialize, Deserializer};

pub(crate) struct RequiredOption<T>(Option<T>);

impl<T> RequiredOption<T> {
    pub(crate) fn into_option(self) -> Option<T> {
        self.0
    }
}

impl<'de, T> Deserialize<'de> for RequiredOption<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum WireRequiredOption<T> {
            Value(T),
            Null(()),
        }

        match WireRequiredOption::deserialize(deserializer)? {
            WireRequiredOption::Value(value) => Ok(Self(Some(value))),
            WireRequiredOption::Null(()) => Ok(Self(None)),
        }
    }
}
