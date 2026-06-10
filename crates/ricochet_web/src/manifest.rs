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
}
