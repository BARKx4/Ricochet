use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscapeMode {
    Html,
    None,
}

pub fn render_template(
    template: &str,
    data: &BTreeMap<String, String>,
    escape: EscapeMode,
) -> Result<String> {
    let mut rendered = String::new();
    let mut pos = 0;

    while pos < template.len() {
        let rest = &template[pos..];
        let next_open = rest.find('{');
        let next_close = rest.find('}');

        match (next_open, next_close) {
            (None, None) => {
                rendered.push_str(rest);
                break;
            }
            (None, Some(_)) => bail!("unmatched closing template brace"),
            (Some(open), Some(close)) if close < open => {
                bail!("unmatched closing template brace")
            }
            (Some(open), _) => {
                rendered.push_str(&rest[..open]);

                let expression_start = pos + open + 1;
                let expression_rest = &template[expression_start..];
                let expression_close = expression_rest
                    .find('}')
                    .ok_or_else(|| anyhow!("unterminated template expression"))?;
                let expression_end = expression_start + expression_close;
                let expression = template[expression_start..expression_end].trim();

                rendered.push_str(&evaluate_expression(expression, data, escape)?);
                pos = expression_end + 1;
            }
        }
    }

    Ok(rendered)
}

fn evaluate_expression(
    expression: &str,
    data: &BTreeMap<String, String>,
    escape: EscapeMode,
) -> Result<String> {
    if expression.is_empty() || expression.contains('{') {
        bail!("unsupported template expression: {expression:?}");
    }

    let tokens = expression.split_whitespace().collect::<Vec<_>>();
    if tokens.len() != 2 || tokens[1] != "get" {
        bail!("unsupported template expression: {expression:?}");
    }

    let key = tokens[0];
    let value = data
        .get(key)
        .ok_or_else(|| anyhow!("missing template value for {key:?}"))?;

    Ok(match escape {
        EscapeMode::Html => escape_html(value),
        EscapeMode::None => value.clone(),
    })
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());

    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }

    escaped
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn template_get_expression_escapes_html_by_default() {
        let mut data = BTreeMap::new();
        data.insert("title".to_string(), "Hello <Ricochet>".to_string());

        let rendered = render_template("<h1>{ title get }</h1>", &data, EscapeMode::Html)
            .expect("template should render");

        assert_eq!(rendered, "<h1>Hello &lt;Ricochet&gt;</h1>");
    }

    #[test]
    fn template_get_expression_can_render_without_escaping() {
        let mut data = BTreeMap::new();
        data.insert("title".to_string(), "Hello <Ricochet>".to_string());

        let rendered = render_template("<h1>{ title get }</h1>", &data, EscapeMode::None)
            .expect("template should render");

        assert_eq!(rendered, "<h1>Hello <Ricochet></h1>");
    }

    #[test]
    fn template_fails_on_unterminated_expression() {
        let data = BTreeMap::new();

        let err = render_template("<h1>{ title get</h1>", &data, EscapeMode::Html)
            .expect_err("unterminated expression should fail");

        assert!(err.to_string().contains("unterminated"));
    }

    #[test]
    fn template_fails_on_unsupported_expression() {
        let data = BTreeMap::new();

        let err = render_template("{ title unknown }", &data, EscapeMode::Html)
            .expect_err("unsupported expression should fail");

        assert!(err.to_string().contains("unsupported"));
    }
}
