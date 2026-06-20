use std::fmt::Write as _;

use anyhow::{anyhow, bail, Result};
use ricochet_syntax::{lex, TokenKind};

pub(crate) fn compile(adapter: &str, source: &str) -> Result<String> {
    let adapter = SqlAdapter::from_name(adapter)?;
    let mut compiler = Compiler::new(adapter);
    compiler.compile(source)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqlAdapter {
    Sqlite,
    Postgres,
    Mysql,
}

impl SqlAdapter {
    fn from_name(name: &str) -> Result<Self> {
        match name {
            "sqlite" => Ok(Self::Sqlite),
            "postgres" | "postgresql" => Ok(Self::Postgres),
            "mysql" | "mariadb" => Ok(Self::Mysql),
            adapter => bail!("migration DSL does not support adapter {:?}", adapter),
        }
    }

    fn quote_identifier(self, identifier: &str) -> String {
        match self {
            Self::Sqlite | Self::Postgres => format!("\"{}\"", identifier.replace('"', "\"\"")),
            Self::Mysql => format!("`{}`", identifier.replace('`', "``")),
        }
    }
}

#[derive(Debug)]
struct Compiler {
    adapter: SqlAdapter,
    stack: Vec<StackString>,
    statements: Vec<Statement>,
    current_create: Option<CreateTable>,
    saw_create: bool,
    saw_drop: bool,
}

impl Compiler {
    fn new(adapter: SqlAdapter) -> Self {
        Self {
            adapter,
            stack: Vec::new(),
            statements: Vec::new(),
            current_create: None,
            saw_create: false,
            saw_drop: false,
        }
    }

    fn compile(&mut self, source: &str) -> Result<String> {
        let tokens =
            lex(source).map_err(|error| anyhow!("failed to lex migration DSL: {error}"))?;
        for token in tokens {
            match token.kind {
                TokenKind::String(value) => self.stack.push(StackString {
                    value,
                    position: token.span.start,
                }),
                TokenKind::Symbol(word) => self.execute_word(&word, token.span.start)?,
                TokenKind::Newline | TokenKind::DocComment(_) | TokenKind::Eof => {}
                other => bail!(
                    "unsupported migration DSL token {:?} at byte {}",
                    other,
                    token.span.start
                ),
            }
        }
        self.finish()
    }

    fn execute_word(&mut self, word: &str, position: usize) -> Result<()> {
        match word {
            "table_create" => self.table_create(position),
            "column" => self.column(position),
            "primary_key" => self.modify_last_column(position, |column| column.primary_key = true),
            "not_null" => self.modify_last_column(position, |column| column.not_null = true),
            "unique" => self.modify_last_column(position, |column| column.unique = true),
            "table_drop" => self.table_drop(position),
            _ => bail!(
                "unsupported migration DSL word {:?} at byte {}",
                word,
                position
            ),
        }
    }

    fn table_create(&mut self, position: usize) -> Result<()> {
        if self.saw_drop {
            bail!(
                "migration DSL cannot mix table_create and table_drop in the same file at byte {}",
                position
            );
        }
        self.finish_current_create()?;
        let table_name = self.pop_string("table_create", position)?;
        validate_identifier("table", &table_name.value)?;
        self.current_create = Some(CreateTable {
            name: table_name.value,
            columns: Vec::new(),
        });
        self.saw_create = true;
        Ok(())
    }

    fn column(&mut self, position: usize) -> Result<()> {
        if self.current_create.is_none() {
            bail!("column before table_create at byte {}", position);
        }
        let type_name = self.pop_string("column type", position)?;
        let column_name = self.pop_string("column name", position)?;
        validate_identifier("column", &column_name.value)?;
        validate_type_name(&type_name.value)?;
        let create = self
            .current_create
            .as_mut()
            .expect("current_create was checked above");
        create.columns.push(Column {
            name: column_name.value,
            type_name: type_name.value,
            primary_key: false,
            not_null: false,
            unique: false,
        });
        Ok(())
    }

    fn modify_last_column<F>(&mut self, position: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut Column),
    {
        let Some(create) = self.current_create.as_mut() else {
            bail!("column modifier before any column at byte {}", position);
        };
        let Some(column) = create.columns.last_mut() else {
            bail!("column modifier before any column at byte {}", position);
        };
        update(column);
        Ok(())
    }

    fn table_drop(&mut self, position: usize) -> Result<()> {
        if self.saw_create || self.current_create.is_some() {
            bail!(
                "migration DSL cannot mix table_create and table_drop in the same file at byte {}",
                position
            );
        }
        let table_name = self.pop_string("table_drop", position)?;
        validate_identifier("table", &table_name.value)?;
        self.statements.push(Statement::DropTable(table_name.value));
        self.saw_drop = true;
        Ok(())
    }

    fn pop_string(&mut self, context: &str, position: usize) -> Result<StackString> {
        self.stack.pop().ok_or_else(|| {
            anyhow!("{context} expected a string literal on the stack at byte {position}")
        })
    }

    fn finish_current_create(&mut self) -> Result<()> {
        if let Some(create) = self.current_create.take() {
            if create.columns.is_empty() {
                bail!("table_create for {:?} has no columns", create.name);
            }
            self.statements.push(Statement::CreateTable(create));
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<String> {
        self.finish_current_create()?;
        if self.statements.is_empty() {
            bail!("migration DSL file contains no migration statements");
        }
        if let Some(value) = self.stack.last() {
            bail!(
                "unused string literal {:?} at byte {}",
                value.value,
                value.position
            );
        }
        self.render_sql()
    }

    fn render_sql(&self) -> Result<String> {
        let mut sql = String::new();
        for (index, statement) in self.statements.iter().enumerate() {
            if index > 0 {
                sql.push('\n');
            }
            match statement {
                Statement::CreateTable(create) => self.render_create_table(&mut sql, create)?,
                Statement::DropTable(table_name) => {
                    writeln!(
                        sql,
                        "drop table {};",
                        self.adapter.quote_identifier(table_name)
                    )?;
                }
            }
        }
        Ok(sql)
    }

    fn render_create_table(&self, sql: &mut String, create: &CreateTable) -> Result<()> {
        writeln!(
            sql,
            "create table {} (",
            self.adapter.quote_identifier(&create.name)
        )?;
        for (index, column) in create.columns.iter().enumerate() {
            write!(
                sql,
                "  {} {}",
                self.adapter.quote_identifier(&column.name),
                column.type_name
            )?;
            if column.primary_key {
                sql.push_str(" primary key");
            }
            if column.not_null {
                sql.push_str(" not null");
            }
            if column.unique {
                sql.push_str(" unique");
            }
            if index + 1 == create.columns.len() {
                sql.push('\n');
            } else {
                sql.push_str(",\n");
            }
        }
        sql.push_str(");\n");
        Ok(())
    }
}

#[derive(Debug)]
struct StackString {
    value: String,
    position: usize,
}

#[derive(Debug)]
enum Statement {
    CreateTable(CreateTable),
    DropTable(String),
}

#[derive(Debug)]
struct CreateTable {
    name: String,
    columns: Vec<Column>,
}

#[derive(Debug)]
struct Column {
    name: String,
    type_name: String,
    primary_key: bool,
    not_null: bool,
    unique: bool,
}

fn validate_identifier(kind: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        bail!("{kind} identifier must be non-empty and cannot have leading or trailing whitespace");
    }
    if value.contains('.') {
        bail!("{kind} identifier must not be schema-qualified");
    }
    if value.chars().any(char::is_control) {
        bail!("{kind} identifier contains a control character");
    }
    Ok(())
}

fn validate_type_name(value: &str) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        bail!("column type must be non-empty and cannot have leading or trailing whitespace");
    }
    if value.contains("--") || value.contains("/*") || value.contains("*/") {
        bail!("column type must not contain SQL comment syntax");
    }
    if value
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '\'' | '"' | '`' | ';' | '\\'))
    {
        bail!("column type contains an unsupported character");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::compile;

    #[test]
    fn compiles_create_table_for_sqlite() {
        let sql = compile(
            "sqlite",
            r#"
"users" table_create
"id" "integer" column primary_key
"email" "text" column not_null unique
"#,
        )
        .expect("migration should compile");

        assert_eq!(
            sql,
            "create table \"users\" (\n  \"id\" integer primary key,\n  \"email\" text not null unique\n);\n"
        );
    }

    #[test]
    fn quotes_adapter_specific_identifiers() {
        let sql = compile(
            "mysql",
            r#"
"we`ird" table_create
"col`umn" "varchar(255)" column
"#,
        )
        .expect("migration should compile");

        assert!(sql.contains("`we``ird`"));
        assert!(sql.contains("`col``umn` varchar(255)"));
    }

    #[test]
    fn rejects_mixed_create_and_drop() {
        let error = compile(
            "sqlite",
            r#"
"users" table_create
"id" "integer" column
"users" table_drop
"#,
        )
        .expect_err("mixed migration should fail")
        .to_string();

        assert!(error.contains("cannot mix table_create and table_drop"));
    }
}
