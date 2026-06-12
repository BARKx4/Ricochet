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

        let api_key = expand_environment_variables(&self.api_key, "AI API key")?;
        if api_key.is_empty() {
            bail!("ai.default.api_key must not resolve to an empty value");
        }

        let base_url = match self.base_url.as_deref() {
            Some(value) => expand_environment_variables(value, "AI base_url")?,
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
}

fn expand_environment_variables(value: &str, context: &str) -> Result<String> {
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
