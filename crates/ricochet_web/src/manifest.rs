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
    #[serde(default)]
    pub session: Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Views {
    pub escape: EscapeMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Session {
    pub signing_secret_env: Option<String>,
}

impl Session {
    pub fn resolved_signing_secret(&self) -> Result<Option<String>> {
        let Some(variable) = &self.signing_secret_env else {
            return Ok(None);
        };
        if variable.is_empty() {
            bail!("session signing_secret_env must not be empty");
        }
        let secret = std::env::var(variable).with_context(|| {
            format!("session signing secret environment variable {variable} is not set")
        })?;
        if secret.is_empty() {
            bail!("session signing secret environment variable {variable} is empty");
        }
        Ok(Some(secret))
    }
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

[web.session]
signing_secret_env = "RICOCHET_SESSION_SECRET"

[database.default]
adapter = "postgres"
url = "postgres://localhost/ricochet_development"
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");

        assert_eq!(manifest.package.name, "web_minimal");
        assert_eq!(manifest.web.mode, "mvc");
        assert_eq!(manifest.web.routes, "config/routes.rco");
        assert_eq!(manifest.web.views.escape, crate::template::EscapeMode::Html);
        assert_eq!(
            manifest.web.session.signing_secret_env.as_deref(),
            Some("RICOCHET_SESSION_SECRET")
        );

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
        assert_eq!(manifest.web.session, Session::default());
    }

    #[test]
    fn session_signing_secret_resolves_environment_variable() {
        std::env::set_var("RICOCHET_TEST_SESSION_SECRET", "test-secret");
        let session = Session {
            signing_secret_env: Some("RICOCHET_TEST_SESSION_SECRET".to_string()),
        };

        let secret = session
            .resolved_signing_secret()
            .expect("session secret should resolve");

        assert_eq!(secret.as_deref(), Some("test-secret"));
    }

    #[test]
    fn session_signing_secret_reports_missing_environment_variable() {
        std::env::remove_var("RICOCHET_MISSING_SESSION_SECRET");
        let session = Session {
            signing_secret_env: Some("RICOCHET_MISSING_SESSION_SECRET".to_string()),
        };

        let error = session
            .resolved_signing_secret()
            .expect_err("missing session secret should fail");

        assert!(
            error
                .to_string()
                .contains("session signing secret environment variable RICOCHET_MISSING_SESSION_SECRET is not set"),
            "{error:#}"
        );
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

        assert!(error.to_string().contains("RICOCHET_MISSING_DATABASE_URL"));
    }
}
