use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMapping {
    pub class_name: String,
    pub table_name: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveRecordError {
    InvalidIdentifier {
        kind: &'static str,
        name: String,
    },
    UnknownField {
        class_name: String,
        field: String,
    },
}

impl fmt::Display for ActiveRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveRecordError::InvalidIdentifier { kind, name } => {
                write!(f, "invalid PostgreSQL {kind} identifier {name:?}")
            }
            ActiveRecordError::UnknownField { class_name, field } => {
                write!(f, "unknown field {field:?} on model {class_name}")
            }
        }
    }
}

impl std::error::Error for ActiveRecordError {}

impl ModelMapping {
    pub fn try_new(
        class_name: impl Into<String>,
        table_name: impl Into<String>,
        fields: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, ActiveRecordError> {
        let table_name = table_name.into();
        validate_table_identifier(&table_name)?;

        let fields = fields
            .into_iter()
            .map(Into::into)
            .map(|field| {
                validate_field_identifier(&field)?;
                Ok(field)
            })
            .collect::<Result<Vec<_>, ActiveRecordError>>()?;

        Ok(Self {
            class_name: class_name.into(),
            table_name,
            fields,
        })
    }

    pub fn select_by_id_sql(&self) -> String {
        format!(
            "select {} from {} where id = $1 limit 1",
            self.fields.join(", "),
            self.table_name
        )
    }

    pub fn select_all_sql(&self) -> String {
        format!("select {} from {}", self.fields.join(", "), self.table_name)
    }

    pub fn select_where_eq_sql(&self, field: &str) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        Ok(format!(
            "select {} from {} where {field} = $1",
            self.fields.join(", "),
            self.table_name
        ))
    }

    pub fn insert_sql(&self) -> String {
        let fields = self.non_id_fields();
        let placeholders = (1..=fields.len())
            .map(|index| format!("${index}"))
            .collect::<Vec<_>>();

        format!(
            "insert into {} ({}) values ({}) returning {}",
            self.table_name,
            fields.join(", "),
            placeholders.join(", "),
            self.fields.join(", ")
        )
    }

    pub fn update_by_id_sql(&self) -> String {
        let fields = self.non_id_fields();
        let assignments = fields
            .iter()
            .enumerate()
            .map(|(index, field)| format!("{field} = ${}", index + 1))
            .collect::<Vec<_>>();
        let id_parameter = fields.len() + 1;

        format!(
            "update {} set {} where id = ${id_parameter} returning {}",
            self.table_name,
            assignments.join(", "),
            self.fields.join(", ")
        )
    }

    fn require_field(&self, field: &str) -> Result<(), ActiveRecordError> {
        validate_field_identifier(field)?;
        if self.fields.iter().any(|known| known == field) {
            Ok(())
        } else {
            Err(ActiveRecordError::UnknownField {
                class_name: self.class_name.clone(),
                field: field.to_string(),
            })
        }
    }

    fn non_id_fields(&self) -> Vec<String> {
        self.fields
            .iter()
            .filter(|field| field.as_str() != "id")
            .cloned()
            .collect()
    }
}

fn validate_table_identifier(name: &str) -> Result<(), ActiveRecordError> {
    if name.split('.').all(is_postgres_identifier) {
        Ok(())
    } else {
        Err(ActiveRecordError::InvalidIdentifier {
            kind: "table",
            name: name.to_string(),
        })
    }
}

fn validate_field_identifier(name: &str) -> Result<(), ActiveRecordError> {
    if is_postgres_identifier(name) {
        Ok(())
    } else {
        Err(ActiveRecordError::InvalidIdentifier {
            kind: "field",
            name: name.to_string(),
        })
    }
}

fn is_postgres_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('_' | 'a'..='z' | 'A'..='Z'))
        && chars.all(|ch| matches!(ch, '_' | 'a'..='z' | 'A'..='Z' | '0'..='9'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_by_id_sql_uses_existing_table_and_fields() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string(), "name".to_string()],
        };

        assert_eq!(
            mapping.select_by_id_sql(),
            "select id, email, name from users where id = $1 limit 1"
        );
    }

    #[test]
    fn select_all_sql_uses_existing_table_and_fields() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(mapping.select_all_sql(), "select id, email from users");
    }

    #[test]
    fn try_new_rejects_unsafe_postgres_identifiers() {
        assert_eq!(
            ModelMapping::try_new("User", "users; drop table users", ["id", "email"]),
            Err(ActiveRecordError::InvalidIdentifier {
                kind: "table",
                name: "users; drop table users".to_string(),
            })
        );
        assert_eq!(
            ModelMapping::try_new("User", "users", ["id", "email-address"]),
            Err(ActiveRecordError::InvalidIdentifier {
                kind: "field",
                name: "email-address".to_string(),
            })
        );
    }

    #[test]
    fn select_where_eq_sql_requires_mapped_field() {
        let mapping = ModelMapping::try_new("User", "public.users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_where_eq_sql("email"),
            Ok("select id, email, name from public.users where email = $1".to_string())
        );
        assert_eq!(
            mapping.select_where_eq_sql("password_hash"),
            Err(ActiveRecordError::UnknownField {
                class_name: "User".to_string(),
                field: "password_hash".to_string(),
            })
        );
    }

    #[test]
    fn insert_and_update_sql_use_non_id_fields_and_return_all_fields() {
        let mapping = ModelMapping::try_new("User", "users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.insert_sql(),
            "insert into users (email, name) values ($1, $2) returning id, email, name"
        );
        assert_eq!(
            mapping.update_by_id_sql(),
            "update users set email = $1, name = $2 where id = $3 returning id, email, name"
        );
    }
}
