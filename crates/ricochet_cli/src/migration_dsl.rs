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

    fn string_literal(self, value: &str) -> String {
        let escaped = match self {
            Self::Sqlite | Self::Postgres => value.replace('\'', "''"),
            Self::Mysql => value.replace('\\', "\\\\").replace('\'', "\\'"),
        };
        format!("'{escaped}'")
    }
}

#[derive(Debug)]
struct Compiler {
    adapter: SqlAdapter,
    stack: Vec<StackString>,
    statements: Vec<Statement>,
    current_create: Option<CreateTable>,
    last_column_target: Option<ColumnTarget>,
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
            last_column_target: None,
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
            "column_add" => self.column_add(position),
            "column_drop" => self.column_drop(position),
            "column_rename" => self.column_rename(position),
            "index_create" => self.index_create(position, false),
            "unique_index_create" => self.index_create(position, true),
            "index_drop" => self.index_drop(position),
            "primary_key" => {
                self.modify_last_column("primary_key", position, |column| column.primary_key = true)
            }
            "not_null" => {
                self.modify_last_column("not_null", position, |column| column.not_null = true)
            }
            "unique" => self.modify_last_column("unique", position, |column| column.unique = true),
            "default" => self.default(position),
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
        self.last_column_target = None;
        self.finish_current_create()?;
        let table_name = self.pop_string("table_create", position)?;
        validate_identifier("table", &table_name.value)?;
        self.current_create = Some(CreateTable {
            name: table_name.value,
            columns: Vec::new(),
        });
        self.last_column_target = None;
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
            default: None,
        });
        self.last_column_target = Some(ColumnTarget::CurrentCreate);
        Ok(())
    }

    fn column_add(&mut self, position: usize) -> Result<()> {
        self.finish_current_create()?;
        let type_name = self.pop_string("column_add type", position)?;
        let column_name = self.pop_string("column_add column name", position)?;
        let table_name = self.pop_string("column_add table", position)?;
        validate_identifier("table", &table_name.value)?;
        validate_identifier("column", &column_name.value)?;
        validate_type_name(&type_name.value)?;

        let statement_index = self.statements.len();
        self.statements.push(Statement::AddColumn(AddColumn {
            table: table_name.value,
            column: Column {
                name: column_name.value,
                type_name: type_name.value,
                primary_key: false,
                not_null: false,
                unique: false,
                default: None,
            },
        }));
        self.last_column_target = Some(ColumnTarget::Statement(statement_index));
        Ok(())
    }

    fn column_drop(&mut self, position: usize) -> Result<()> {
        self.finish_current_create()?;
        self.last_column_target = None;
        let column_name = self.pop_string("column_drop column name", position)?;
        let table_name = self.pop_string("column_drop table", position)?;
        validate_identifier("table", &table_name.value)?;
        validate_identifier("column", &column_name.value)?;
        self.statements.push(Statement::DropColumn(DropColumn {
            table: table_name.value,
            column: column_name.value,
        }));
        Ok(())
    }

    fn column_rename(&mut self, position: usize) -> Result<()> {
        self.finish_current_create()?;
        self.last_column_target = None;
        let new_name = self.pop_string("column_rename new column name", position)?;
        let old_name = self.pop_string("column_rename old column name", position)?;
        let table_name = self.pop_string("column_rename table", position)?;
        validate_identifier("table", &table_name.value)?;
        validate_identifier("column", &old_name.value)?;
        validate_identifier("column", &new_name.value)?;
        self.statements.push(Statement::RenameColumn(RenameColumn {
            table: table_name.value,
            old: old_name.value,
            new: new_name.value,
        }));
        Ok(())
    }

    fn index_create(&mut self, position: usize, unique: bool) -> Result<()> {
        self.finish_current_create()?;
        self.last_column_target = None;
        let column_name = self.pop_string("index_create column name", position)?;
        let table_name = self.pop_string("index_create table", position)?;
        let index_name = self.pop_string("index_create index name", position)?;
        validate_identifier("index", &index_name.value)?;
        validate_identifier("table", &table_name.value)?;
        validate_identifier("column", &column_name.value)?;
        self.statements.push(Statement::CreateIndex(CreateIndex {
            name: index_name.value,
            table: table_name.value,
            column: column_name.value,
            unique,
        }));
        Ok(())
    }

    fn index_drop(&mut self, position: usize) -> Result<()> {
        self.finish_current_create()?;
        self.last_column_target = None;
        let table_name = self.pop_string("index_drop table", position)?;
        let index_name = self.pop_string("index_drop index name", position)?;
        validate_identifier("index", &index_name.value)?;
        validate_identifier("table", &table_name.value)?;
        self.statements.push(Statement::DropIndex(DropIndex {
            name: index_name.value,
            table: table_name.value,
        }));
        Ok(())
    }

    fn modify_last_column<F>(&mut self, word: &str, position: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut Column),
    {
        match self.last_column_target {
            Some(ColumnTarget::CurrentCreate) => {
                let Some(create) = self.current_create.as_mut() else {
                    bail!(
                        "{word} modifier target is no longer a column or column_add at byte {}",
                        position
                    );
                };
                let Some(column) = create.columns.last_mut() else {
                    bail!(
                        "{word} modifier before any column or column_add at byte {}",
                        position
                    );
                };
                update(column);
            }
            Some(ColumnTarget::Statement(index)) => {
                let Some(Statement::AddColumn(add_column)) = self.statements.get_mut(index) else {
                    bail!(
                        "{word} modifier target is no longer a column or column_add at byte {}",
                        position
                    );
                };
                update(&mut add_column.column);
            }
            None => {
                bail!(
                    "{word} modifier before any column or column_add at byte {}",
                    position
                );
            }
        };
        Ok(())
    }

    fn default(&mut self, position: usize) -> Result<()> {
        if self.last_column_target.is_none() {
            bail!(
                "default modifier before any column or column_add at byte {}",
                position
            );
        }
        let literal = self.pop_string("default literal", position)?;
        self.modify_last_column("default", position, |column| {
            column.default = Some(literal.value);
        })
    }

    fn table_drop(&mut self, position: usize) -> Result<()> {
        if self.saw_create || self.current_create.is_some() {
            bail!(
                "migration DSL cannot mix table_create and table_drop in the same file at byte {}",
                position
            );
        }
        self.last_column_target = None;
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
                Statement::AddColumn(add_column) => {
                    self.validate_add_column(add_column)?;
                    write!(
                        sql,
                        "alter table {} add column ",
                        self.adapter.quote_identifier(&add_column.table)
                    )?;
                    self.render_column_definition(&mut sql, &add_column.column)?;
                    sql.push_str(";\n");
                }
                Statement::DropColumn(drop_column) => {
                    writeln!(
                        sql,
                        "alter table {} drop column {};",
                        self.adapter.quote_identifier(&drop_column.table),
                        self.adapter.quote_identifier(&drop_column.column)
                    )?;
                }
                Statement::RenameColumn(rename_column) => {
                    writeln!(
                        sql,
                        "alter table {} rename column {} to {};",
                        self.adapter.quote_identifier(&rename_column.table),
                        self.adapter.quote_identifier(&rename_column.old),
                        self.adapter.quote_identifier(&rename_column.new)
                    )?;
                }
                Statement::CreateIndex(create_index) => {
                    if create_index.unique {
                        write!(sql, "create unique index ")?;
                    } else {
                        write!(sql, "create index ")?;
                    }
                    writeln!(
                        sql,
                        "{} on {} ({});",
                        self.adapter.quote_identifier(&create_index.name),
                        self.adapter.quote_identifier(&create_index.table),
                        self.adapter.quote_identifier(&create_index.column)
                    )?;
                }
                Statement::DropIndex(drop_index) => match self.adapter {
                    SqlAdapter::Sqlite | SqlAdapter::Postgres => {
                        writeln!(
                            sql,
                            "drop index {};",
                            self.adapter.quote_identifier(&drop_index.name)
                        )?;
                    }
                    SqlAdapter::Mysql => {
                        writeln!(
                            sql,
                            "drop index {} on {};",
                            self.adapter.quote_identifier(&drop_index.name),
                            self.adapter.quote_identifier(&drop_index.table)
                        )?;
                    }
                },
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

    fn validate_add_column(&self, add_column: &AddColumn) -> Result<()> {
        if self.adapter != SqlAdapter::Sqlite {
            return Ok(());
        }
        if add_column.column.primary_key {
            bail!("SQLite ADD COLUMN does not support primary_key");
        }
        if add_column.column.unique {
            bail!("SQLite ADD COLUMN does not support unique");
        }
        if add_column.column.not_null && add_column.column.default.is_none() {
            bail!("SQLite ADD COLUMN not_null requires a default");
        }
        Ok(())
    }

    fn render_create_table(&self, sql: &mut String, create: &CreateTable) -> Result<()> {
        writeln!(
            sql,
            "create table {} (",
            self.adapter.quote_identifier(&create.name)
        )?;
        for (index, column) in create.columns.iter().enumerate() {
            sql.push_str("  ");
            self.render_column_definition(sql, column)?;
            if index + 1 == create.columns.len() {
                sql.push('\n');
            } else {
                sql.push_str(",\n");
            }
        }
        sql.push_str(");\n");
        Ok(())
    }

    fn render_column_definition(&self, sql: &mut String, column: &Column) -> Result<()> {
        write!(
            sql,
            "{} {}",
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
        if let Some(default) = &column.default {
            write!(sql, " default {}", self.adapter.string_literal(default))?;
        }
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
    AddColumn(AddColumn),
    DropColumn(DropColumn),
    RenameColumn(RenameColumn),
    CreateIndex(CreateIndex),
    DropIndex(DropIndex),
    DropTable(String),
}

#[derive(Debug, Clone, Copy)]
enum ColumnTarget {
    CurrentCreate,
    Statement(usize),
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
    default: Option<String>,
}

#[derive(Debug)]
struct AddColumn {
    table: String,
    column: Column,
}

#[derive(Debug)]
struct DropColumn {
    table: String,
    column: String,
}

#[derive(Debug)]
struct RenameColumn {
    table: String,
    old: String,
    new: String,
}

#[derive(Debug)]
struct CreateIndex {
    name: String,
    table: String,
    column: String,
    unique: bool,
}

#[derive(Debug)]
struct DropIndex {
    name: String,
    table: String,
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

    #[test]
    fn compiles_alter_column_and_index_operations_for_postgres() {
        let sql = compile(
            "postgres",
            r#"
"users" "nickname" "text" column_add not_null "guest's" default
"users" "nickname" "display_name" column_rename
"idx_users_display_name" "users" "display_name" index_create
"uq_users_display_name" "users" "display_name" unique_index_create
"idx_users_display_name" "users" index_drop
"users" "display_name" column_drop
"#,
        )
        .expect("migration should compile");

        assert_eq!(
            sql,
            "alter table \"users\" add column \"nickname\" text not null default 'guest''s';\n\
\n\
alter table \"users\" rename column \"nickname\" to \"display_name\";\n\
\n\
create index \"idx_users_display_name\" on \"users\" (\"display_name\");\n\
\n\
create unique index \"uq_users_display_name\" on \"users\" (\"display_name\");\n\
\n\
drop index \"idx_users_display_name\";\n\
\n\
alter table \"users\" drop column \"display_name\";\n"
        );
    }

    #[test]
    fn index_drop_is_adapter_specific() {
        let source = r#"
"idx_users_email" "users" index_drop
"#;

        assert_eq!(
            compile("sqlite", source).expect("sqlite migration should compile"),
            "drop index \"idx_users_email\";\n"
        );
        assert_eq!(
            compile("postgres", source).expect("postgres migration should compile"),
            "drop index \"idx_users_email\";\n"
        );
        assert_eq!(
            compile("mysql", source).expect("mysql migration should compile"),
            "drop index `idx_users_email` on `users`;\n"
        );
    }

    #[test]
    fn default_modifier_works_for_create_table_columns() {
        let sql = compile(
            "sqlite",
            r#"
"users" table_create
"nickname" "text" column "O'Reilly" default
"#,
        )
        .expect("migration should compile");

        assert_eq!(
            sql,
            "create table \"users\" (\n  \"nickname\" text default 'O''Reilly'\n);\n"
        );
    }

    #[test]
    fn sqlite_add_column_rejects_primary_key_and_unique() {
        for (source, expected) in [
            (
                r#""users" "id" "integer" column_add primary_key"#,
                "SQLite ADD COLUMN does not support primary_key",
            ),
            (
                r#""users" "email" "text" column_add unique"#,
                "SQLite ADD COLUMN does not support unique",
            ),
        ] {
            let error = compile("sqlite", source)
                .expect_err("SQLite add column should reject unsupported modifier")
                .to_string();
            assert!(
                error.contains(expected),
                "expected {expected:?} in {error:?}"
            );
        }
    }

    #[test]
    fn sqlite_add_column_rejects_not_null_without_default() {
        let error = compile("sqlite", r#""users" "email" "text" column_add not_null"#)
            .expect_err("SQLite add column should reject not_null without default")
            .to_string();

        assert!(error.contains("SQLite ADD COLUMN not_null requires a default"));
    }

    #[test]
    fn sqlite_add_column_accepts_not_null_with_default() {
        let sql = compile(
            "sqlite",
            r#""users" "email" "text" column_add not_null "ada@example.com" default"#,
        )
        .expect("SQLite add column should accept not_null with default");

        assert_eq!(
            sql,
            "alter table \"users\" add column \"email\" text not null default 'ada@example.com';\n"
        );
    }

    #[test]
    fn mysql_default_escapes_quote_backslash_and_keeps_semicolon() {
        let sql = compile(
            "mysql",
            r#""users" "path" "varchar(255)" column_add "O'Reilly C:\\tmp;done" default"#,
        )
        .expect("MySQL migration should compile");

        assert_eq!(
            sql,
            "alter table `users` add column `path` varchar(255) default 'O\\'Reilly C:\\\\tmp;done';\n"
        );
    }

    #[test]
    fn modifiers_fail_before_any_column_or_column_add() {
        for (source, expected) in [
            (
                "primary_key",
                "primary_key modifier before any column or column_add",
            ),
            (
                "not_null",
                "not_null modifier before any column or column_add",
            ),
            ("unique", "unique modifier before any column or column_add"),
            (
                r#""guest" default"#,
                "default modifier before any column or column_add",
            ),
        ] {
            let error = compile("sqlite", source)
                .expect_err("modifier before column should fail")
                .to_string();
            assert!(
                error.contains(expected),
                "expected {expected:?} in {error:?}"
            );
        }
    }
}
