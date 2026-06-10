#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMapping {
    pub class_name: String,
    pub table_name: String,
    pub fields: Vec<String>,
}

impl ModelMapping {
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
}
