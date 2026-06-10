use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::template::EscapeMode;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub package: Package,
    pub web: Web,
    #[serde(default)]
    pub database: Database,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Package {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Web {
    pub mode: String,
    pub routes: String,
    pub views: Views,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Views {
    pub escape: EscapeMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Database {
    pub default: Option<DatabaseDefault>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DatabaseDefault {
    pub adapter: String,
    pub url: String,
}

impl DatabaseDefault {
    pub fn resolved_url(&self) -> Result<String> {
        expand_environment_variables(&self.url)
    }
}

fn expand_environment_variables(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut remaining = value;

    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let variable_start = start + 2;
        let Some(relative_end) = remaining[variable_start..].find('}') else {
            bail!("unterminated environment variable in database URL");
        };
        let variable_end = variable_start + relative_end;
        let variable = &remaining[variable_start..variable_end];
        if variable.is_empty() {
            bail!("empty environment variable in database URL");
        }
        let replacement = std::env::var(variable)
            .with_context(|| format!("database URL environment variable {variable} is not set"))?;
        output.push_str(&replacement);
        remaining = &remaining[variable_end + 1..];
    }

    output.push_str(remaining);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_planned_mvc_postgres_manifest() {
        let source = r#"
[package]
name = "web_minimal"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "postgres://localhost/ricochet_development"
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");

        assert_eq!(manifest.package.name, "web_minimal");
        assert_eq!(manifest.web.mode, "mvc");
        assert_eq!(manifest.web.routes, "config/routes.rco");
        assert_eq!(manifest.web.views.escape, crate::template::EscapeMode::Html);

        let database = manifest
            .database
            .default
            .expect("postgres default database should be present");
        assert_eq!(database.adapter, "postgres");
        assert_eq!(database.url, "postgres://localhost/ricochet_development");
    }

    #[test]
    fn manifest_defaults_database_when_section_is_missing() {
        let source = r#"
[package]
name = "web_minimal"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "none"
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");

        assert!(manifest.database.default.is_none());
        assert_eq!(manifest.web.views.escape, crate::template::EscapeMode::None);
    }

    #[test]
    fn database_url_expands_environment_variables() {
        std::env::set_var(
            "RICOCHET_TEST_DATABASE_URL",
            "postgres://localhost/ricochet_test",
        );
        let database = DatabaseDefault {
            adapter: "postgres".to_string(),
            url: "${RICOCHET_TEST_DATABASE_URL}".to_string(),
        };

        let resolved = database
            .resolved_url()
            .expect("database URL should resolve");

        assert_eq!(resolved, "postgres://localhost/ricochet_test");
    }

    #[test]
    fn database_url_reports_missing_environment_variables() {
        std::env::remove_var("RICOCHET_MISSING_DATABASE_URL");
        let database = DatabaseDefault {
            adapter: "postgres".to_string(),
            url: "${RICOCHET_MISSING_DATABASE_URL}".to_string(),
        };

        let error = database
            .resolved_url()
            .expect_err("missing variable should fail");

        assert!(error
            .to_string()
            .contains("RICOCHET_MISSING_DATABASE_URL"));
    }
}
