use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::ai_capability::AiProviderConfig;
use crate::template::EscapeMode;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub package: Package,
    pub web: Web,
    #[serde(default)]
    pub database: Database,
    #[serde(default)]
    pub ai: Ai,
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
    #[serde(default = "default_controller_instruction_limit")]
    pub controller_instruction_limit: u64,
    #[serde(default)]
    pub uploads: Uploads,
    #[serde(default, rename = "static")]
    pub static_files: StaticFiles,
    #[serde(default)]
    pub session: Session,
    #[serde(default)]
    pub capabilities: WebCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StaticFiles {
    #[serde(default = "default_static_dir")]
    pub dir: String,
    #[serde(default = "default_static_mount")]
    pub mount: String,
}

impl Default for StaticFiles {
    fn default() -> Self {
        Self {
            dir: default_static_dir(),
            mount: default_static_mount(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Uploads {
    #[serde(default = "default_upload_max_request_bytes")]
    pub max_request_bytes: usize,
    #[serde(default = "default_upload_max_file_bytes")]
    pub max_file_bytes: usize,
    #[serde(default = "default_upload_memory_threshold_bytes")]
    pub memory_threshold_bytes: usize,
    #[serde(default = "default_upload_max_retained_streams")]
    pub max_retained_streams: usize,
}

impl Default for Uploads {
    fn default() -> Self {
        Self {
            max_request_bytes: default_upload_max_request_bytes(),
            max_file_bytes: default_upload_max_file_bytes(),
            memory_threshold_bytes: default_upload_memory_threshold_bytes(),
            max_retained_streams: default_upload_max_retained_streams(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Views {
    pub escape: EscapeMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct WebCapabilities {
    pub fs_root: Option<String>,
    pub fs_readonly: bool,
    pub allow_env: bool,
    pub env_allow: Vec<String>,
    pub allow_process: bool,
    pub process_root: Option<String>,
    pub allow_pty: bool,
    pub http_allow_hosts: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Session {
    pub signing_secret_env: Option<String>,
    pub encryption_secret_env: Option<String>,
    #[serde(default)]
    pub secure: SessionSecure,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionSecure {
    #[default]
    Auto,
    Always,
    Never,
}

fn default_static_dir() -> String {
    "public".to_string()
}

fn default_static_mount() -> String {
    "/assets".to_string()
}

fn default_upload_max_request_bytes() -> usize {
    16 * 1024 * 1024
}

pub(crate) fn default_controller_instruction_limit() -> u64 {
    250_000
}

fn default_upload_max_file_bytes() -> usize {
    16 * 1024 * 1024
}

fn default_upload_memory_threshold_bytes() -> usize {
    1024 * 1024
}

fn default_upload_max_retained_streams() -> usize {
    64
}

impl Session {
    pub fn resolved_signing_secret(&self) -> Result<Option<String>> {
        self.resolve_secret(
            self.signing_secret_env.as_deref(),
            "signing_secret_env",
            "signing secret",
        )
    }

    pub fn resolved_encryption_secret(&self) -> Result<Option<String>> {
        self.resolve_secret(
            self.encryption_secret_env.as_deref(),
            "encryption_secret_env",
            "encryption secret",
        )
    }

    fn resolve_secret(
        &self,
        variable: Option<&str>,
        field: &str,
        description: &str,
    ) -> Result<Option<String>> {
        let Some(variable) = variable else {
            return Ok(None);
        };
        if variable.is_empty() {
            bail!("session {field} must not be empty");
        }
        let secret = std::env::var(variable).with_context(|| {
            format!("session {description} environment variable {variable} is not set")
        })?;
        if secret.is_empty() {
            bail!("session {description} environment variable {variable} is empty");
        }
        Ok(Some(secret))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Database {
    pub default: Option<DatabaseDefault>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Ai {
    pub default: Option<AiDefault>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AiDefault {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl AiDefault {
    pub fn resolved_config(&self) -> Result<AiProviderConfig> {
        self.resolved_config_with_env_policy(None)
    }

    pub fn resolved_config_with_env_policy(
        &self,
        allowed_env: Option<&BTreeSet<String>>,
    ) -> Result<AiProviderConfig> {
        let provider = self.provider.trim();
        if provider.is_empty() {
            bail!("ai.default.provider must not be empty");
        }
        if !matches!(provider, "openai" | "openai-compatible") {
            bail!("unsupported AI provider {provider}; expected openai or openai-compatible");
        }

        let model = self.model.trim();
        if model.is_empty() {
            bail!("ai.default.model must not be empty");
        }

        let api_key =
            expand_environment_variables_with_policy(&self.api_key, "AI API key", allowed_env)?;
        if api_key.is_empty() {
            bail!("ai.default.api_key must not resolve to an empty value");
        }

        let base_url = match self.base_url.as_deref() {
            Some(value) => {
                expand_environment_variables_with_policy(value, "AI base_url", allowed_env)?
            }
            None if provider == "openai" => "https://api.openai.com/v1".to_string(),
            None => bail!("ai.default.base_url is required for openai-compatible provider"),
        };
        if base_url.trim().is_empty() {
            bail!("ai.default.base_url must not resolve to an empty value");
        }

        Ok(AiProviderConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            api_key,
            base_url,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DatabaseDefault {
    pub adapter: String,
    pub url: String,
}

impl DatabaseDefault {
    pub fn resolved_url(&self) -> Result<String> {
        expand_environment_variables(&self.url, "database URL")
    }

    pub fn resolved_url_with_env_policy(
        &self,
        allowed_env: Option<&BTreeSet<String>>,
    ) -> Result<String> {
        expand_environment_variables_with_policy(&self.url, "database URL", allowed_env)
    }
}

fn expand_environment_variables(value: &str, context: &str) -> Result<String> {
    expand_environment_variables_with_policy(value, context, None)
}

fn expand_environment_variables_with_policy(
    value: &str,
    context: &str,
    allowed_env: Option<&BTreeSet<String>>,
) -> Result<String> {
    let mut output = String::new();
    let mut remaining = value;

    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let variable_start = start + 2;
        let Some(relative_end) = remaining[variable_start..].find('}') else {
            bail!("unterminated environment variable in {context}");
        };
        let variable_end = variable_start + relative_end;
        let variable = &remaining[variable_start..variable_end];
        if variable.is_empty() {
            bail!("empty environment variable in {context}");
        }
        if let Some(allowed_env) = allowed_env {
            if !allowed_env.contains(variable) {
                bail!("{context} environment variable {variable} is not allowed by serve policy");
            }
        }
        let replacement = std::env::var(variable)
            .with_context(|| format!("{context} environment variable {variable} is not set"))?;
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
encryption_secret_env = "RICOCHET_SESSION_ENCRYPTION_SECRET"

[database.default]
adapter = "postgres"
url = "postgres://localhost/ricochet_development"

[ai.default]
provider = "openai"
model = "gpt-4.1-mini"
api_key = "${RICOCHET_TEST_OPENAI_API_KEY}"
"#;

        std::env::set_var("RICOCHET_TEST_OPENAI_API_KEY", "test-key");
        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");

        assert_eq!(manifest.package.name, "web_minimal");
        assert_eq!(manifest.web.mode, "mvc");
        assert_eq!(manifest.web.routes, "config/routes.rco");
        assert_eq!(manifest.web.views.escape, crate::template::EscapeMode::Html);
        assert_eq!(manifest.web.controller_instruction_limit, 250_000);
        assert_eq!(manifest.web.uploads, Uploads::default());
        assert_eq!(manifest.web.static_files, StaticFiles::default());
        assert_eq!(
            manifest.web.session.signing_secret_env.as_deref(),
            Some("RICOCHET_SESSION_SECRET")
        );
        assert_eq!(
            manifest.web.session.encryption_secret_env.as_deref(),
            Some("RICOCHET_SESSION_ENCRYPTION_SECRET")
        );

        let database = manifest
            .database
            .default
            .expect("postgres default database should be present");
        assert_eq!(database.adapter, "postgres");
        assert_eq!(database.url, "postgres://localhost/ricochet_development");

        let ai = manifest
            .ai
            .default
            .expect("default AI provider should be present");
        assert_eq!(ai.provider, "openai");
        assert_eq!(ai.model, "gpt-4.1-mini");
        assert_eq!(ai.api_key, "${RICOCHET_TEST_OPENAI_API_KEY}");
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
        assert!(manifest.ai.default.is_none());
        assert_eq!(manifest.web.views.escape, crate::template::EscapeMode::None);
        assert_eq!(manifest.web.controller_instruction_limit, 250_000);
        assert_eq!(manifest.web.uploads, Uploads::default());
        assert_eq!(manifest.web.static_files.dir, "public");
        assert_eq!(manifest.web.static_files.mount, "/assets");
        assert_eq!(manifest.web.session, Session::default());
    }

    #[test]
    fn manifest_parses_static_asset_config() {
        let source = r#"
[package]
name = "static_app"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.static]
dir = "frontend/dist"
mount = "/static"
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");

        assert_eq!(manifest.web.static_files.dir, "frontend/dist");
        assert_eq!(manifest.web.static_files.mount, "/static");
    }

    #[test]
    fn manifest_parses_upload_limits() {
        let source = r#"
[package]
name = "upload_app"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.uploads]
max_request_bytes = 8388608
max_file_bytes = 4194304
memory_threshold_bytes = 4096
max_retained_streams = 8
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");

        assert_eq!(
            manifest.web.uploads,
            Uploads {
                max_request_bytes: 8_388_608,
                max_file_bytes: 4_194_304,
                memory_threshold_bytes: 4096,
                max_retained_streams: 8,
            }
        );
    }

    #[test]
    fn manifest_parses_controller_instruction_limit() {
        let source = r#"
[package]
name = "budgeted_app"

[web]
mode = "mvc"
routes = "config/routes.rco"
controller_instruction_limit = 400000

[web.views]
escape = "html"
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");

        assert_eq!(manifest.web.controller_instruction_limit, 400_000);
    }

    #[test]
    fn manifest_parses_sqlite_database_adapter() {
        let source = r#"
[package]
name = "sqlite_app"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");
        let database = manifest
            .database
            .default
            .expect("sqlite default database should be present");

        assert_eq!(database.adapter, "sqlite");
        assert_eq!(database.url, "db/development.sqlite3");
    }

    #[test]
    fn manifest_parses_mysql_database_adapter() {
        let source = r#"
[package]
name = "mysql_app"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "mysql"
url = "${MYSQL_URL}"
"#;

        let manifest: Manifest = toml::from_str(source).expect("manifest should parse");
        let database = manifest
            .database
            .default
            .expect("mysql default database should be present");

        assert_eq!(database.adapter, "mysql");
        assert_eq!(database.url, "${MYSQL_URL}");
    }

    #[test]
    fn session_signing_secret_resolves_environment_variable() {
        std::env::set_var("RICOCHET_TEST_SESSION_SECRET", "test-secret");
        let session = Session {
            signing_secret_env: Some("RICOCHET_TEST_SESSION_SECRET".to_string()),
            encryption_secret_env: None,
            secure: SessionSecure::Auto,
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
            encryption_secret_env: None,
            secure: SessionSecure::Auto,
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
    fn session_encryption_secret_resolves_environment_variable() {
        std::env::set_var("RICOCHET_TEST_SESSION_ENCRYPTION_SECRET", "test-secret");
        let session = Session {
            signing_secret_env: None,
            encryption_secret_env: Some("RICOCHET_TEST_SESSION_ENCRYPTION_SECRET".to_string()),
            secure: SessionSecure::Auto,
        };

        let secret = session
            .resolved_encryption_secret()
            .expect("session encryption secret should resolve");

        assert_eq!(secret.as_deref(), Some("test-secret"));
    }

    #[test]
    fn session_encryption_secret_reports_missing_environment_variable() {
        std::env::remove_var("RICOCHET_MISSING_SESSION_ENCRYPTION_SECRET");
        let session = Session {
            signing_secret_env: None,
            encryption_secret_env: Some("RICOCHET_MISSING_SESSION_ENCRYPTION_SECRET".to_string()),
            secure: SessionSecure::Auto,
        };

        let error = session
            .resolved_encryption_secret()
            .expect_err("missing session encryption secret should fail");

        assert!(
            error.to_string().contains(
                "session encryption secret environment variable RICOCHET_MISSING_SESSION_ENCRYPTION_SECRET is not set"
            ),
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

    #[test]
    fn ai_provider_config_resolves_environment_variables() {
        std::env::set_var("RICOCHET_TEST_AI_KEY", "test-ai-key");
        let ai = AiDefault {
            provider: "openai".to_string(),
            model: "gpt-4.1-mini".to_string(),
            api_key: "${RICOCHET_TEST_AI_KEY}".to_string(),
            base_url: None,
        };

        let config = ai.resolved_config().expect("AI config should resolve");

        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4.1-mini");
        assert_eq!(config.api_key, "test-ai-key");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn ai_provider_config_reports_missing_environment_variable() {
        std::env::remove_var("RICOCHET_MISSING_AI_KEY");
        let ai = AiDefault {
            provider: "openai".to_string(),
            model: "gpt-4.1-mini".to_string(),
            api_key: "${RICOCHET_MISSING_AI_KEY}".to_string(),
            base_url: None,
        };

        let error = ai
            .resolved_config()
            .expect_err("missing AI key should fail");

        assert!(error.to_string().contains("RICOCHET_MISSING_AI_KEY"));
    }

    #[test]
    fn ai_provider_config_requires_base_url_for_openai_compatible() {
        let ai = AiDefault {
            provider: "openai-compatible".to_string(),
            model: "local-model".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
        };

        let error = ai
            .resolved_config()
            .expect_err("compatible provider needs an endpoint");

        assert!(error.to_string().contains("base_url is required"));
    }
}
