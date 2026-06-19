use std::{borrow::Cow, collections::BTreeMap};

use anyhow::{anyhow, bail, Context, Result};
use ricochet_compiler::compile_source;
use ricochet_vm::{Value, Vm};
use serde::Deserialize;

const TEMPLATE_INSTRUCTION_LIMIT: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscapeMode {
    Html,
    None,
}

pub fn render_template(
    template: &str,
    data: &BTreeMap<String, Value>,
    escape: EscapeMode,
) -> Result<String> {
    let template = normalize_template_newlines(template);
    let template = template.as_ref();
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

                ensure_text_context(template, pos + open)?;
                rendered.push_str(&evaluate_expression(expression, data, escape)?);
                pos = expression_end + 1;
            }
        }
    }

    Ok(rendered)
}

fn normalize_template_newlines(template: &str) -> Cow<'_, str> {
    if template.contains('\r') {
        Cow::Owned(template.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(template)
    }
}

fn ensure_text_context(template: &str, open_index: usize) -> Result<()> {
    let before = &template[..open_index];
    let lower_before = before.to_ascii_lowercase();
    if unclosed_tag_before(&lower_before, "<script", "</script") {
        bail!("template expressions are not allowed inside script blocks");
    }
    if unclosed_tag_before(&lower_before, "<style", "</style") {
        bail!("template expressions are not allowed inside style blocks");
    }
    let last_open = before.rfind('<');
    let last_close = before.rfind('>');
    if last_open.is_some() && last_open > last_close {
        bail!("template expressions are not allowed inside HTML tags or attributes");
    }
    Ok(())
}

fn unclosed_tag_before(source: &str, open: &str, close: &str) -> bool {
    match source.rfind(open) {
        Some(open_index) => source
            .rfind(close)
            .is_none_or(|close_index| close_index < open_index),
        None => false,
    }
}

fn evaluate_expression(
    expression: &str,
    data: &BTreeMap<String, Value>,
    escape: EscapeMode,
) -> Result<String> {
    if expression.is_empty() || expression.contains('{') {
        bail!("unsupported template expression: {expression:?}");
    }

    let chunk = compile_source("<template>", expression)
        .with_context(|| format!("failed to compile template expression {expression:?}"))?;
    let mut vm = Vm::default();
    vm.set_instruction_limit(TEMPLATE_INSTRUCTION_LIMIT);
    for (name, value) in data {
        vm.set_variable(name.clone(), value.clone());
    }
    vm.run_chunk(&chunk)
        .with_context(|| format!("failed to execute template expression {expression:?}"))?;

    if vm.stack().len() != 1 {
        bail!(
            "template expression {expression:?} must leave exactly one value, left {}",
            vm.stack().len()
        );
    }
    let value = vm
        .stack()
        .last()
        .expect("template stack length checked before reading");
    let value = render_value(value)?;

    Ok(match escape {
        EscapeMode::Html => escape_html(&value),
        EscapeMode::None => value,
    })
}

fn render_value(value: &Value) -> Result<String> {
    match value {
        Value::Nil => Ok(String::new()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Float(value) => Ok(format_float(*value)),
        Value::String(value) => Ok(value.clone()),
        value => bail!("template expression returned non-renderable value {value:?}"),
    }
}

fn format_float(value: f64) -> String {
    let formatted = value.to_string();
    if !value.is_finite() {
        return formatted;
    }
    if formatted.contains('.') || formatted.contains('e') || formatted.contains('E') {
        formatted
    } else {
        format!("{formatted}.0")
    }
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
        data.insert(
            "title".to_string(),
            Value::String("Hello <Ricochet>".to_string()),
        );

        let rendered = render_template("<h1>{ title get }</h1>", &data, EscapeMode::Html)
            .expect("template should render");

        assert_eq!(rendered, "<h1>Hello &lt;Ricochet&gt;</h1>");
    }

    #[test]
    fn template_get_expression_can_render_without_escaping() {
        let mut data = BTreeMap::new();
        data.insert(
            "title".to_string(),
            Value::String("Hello <Ricochet>".to_string()),
        );

        let rendered = render_template("<h1>{ title get }</h1>", &data, EscapeMode::None)
            .expect("template should render");

        assert_eq!(rendered, "<h1>Hello <Ricochet></h1>");
    }

    #[test]
    fn template_executes_postfix_ricochet_expressions() {
        let data = BTreeMap::new();

        let rendered = render_template("<p>{ 20 22 + }</p>", &data, EscapeMode::Html)
            .expect("template should execute Ricochet");

        assert_eq!(rendered, "<p>42</p>");
    }

    #[test]
    fn template_normalizes_windows_newlines() {
        let data = BTreeMap::from([(
            "title".to_string(),
            Value::String("Hello <Ricochet>".to_string()),
        )]);

        let rendered = render_template(
            "<h1>{ title get }</h1>\r\n<p>static</p>\r\n",
            &data,
            EscapeMode::Html,
        )
        .expect("template should render");

        assert_eq!(rendered, "<h1>Hello &lt;Ricochet&gt;</h1>\n<p>static</p>\n");
    }

    #[test]
    fn template_expressions_can_navigate_nested_values() {
        let data = BTreeMap::from([(
            "user".to_string(),
            Value::Map(
                BTreeMap::from([(
                    "name".to_string(),
                    Value::String("Ada <Lovelace>".to_string()),
                )])
                .into(),
            ),
        )]);

        let rendered = render_template(
            "<strong>{ user get \"name\" at }</strong>",
            &data,
            EscapeMode::Html,
        )
        .expect("template should navigate map values");

        assert_eq!(rendered, "<strong>Ada &lt;Lovelace&gt;</strong>");
    }

    #[test]
    fn template_expressions_fail_inside_attributes() {
        let data = BTreeMap::from([("url".to_string(), Value::String("/safe".to_string()))]);

        let error = render_template("<a href=\"{ url get }\">link</a>", &data, EscapeMode::Html)
            .expect_err("attribute interpolation should fail closed");

        assert!(
            error
                .to_string()
                .contains("template expressions are not allowed inside HTML tags or attributes"),
            "error was {error:#}"
        );
    }

    #[test]
    fn template_expressions_fail_inside_script_blocks() {
        let data = BTreeMap::from([("name".to_string(), Value::String("Ada".to_string()))]);

        let error = render_template("<script>{ name get }</script>", &data, EscapeMode::Html)
            .expect_err("script interpolation should fail closed");

        assert!(
            error
                .to_string()
                .contains("template expressions are not allowed inside script blocks"),
            "error was {error:#}"
        );
    }

    #[test]
    fn template_expression_requires_exactly_one_stack_result() {
        let data = BTreeMap::new();

        let err = render_template("{ 1 2 }", &data, EscapeMode::Html)
            .expect_err("extra stack values should fail");

        assert!(err.to_string().contains("exactly one value"));
    }

    #[test]
    fn template_fails_on_unterminated_expression() {
        let data = BTreeMap::new();

        let err = render_template("<h1>{ title get</h1>", &data, EscapeMode::Html)
            .expect_err("unterminated expression should fail");

        assert!(err.to_string().contains("unterminated"));
    }

    #[test]
    fn template_fails_when_ricochet_expression_faults() {
        let data = BTreeMap::new();

        let err = render_template("{ title unknown }", &data, EscapeMode::Html)
            .expect_err("faulting expression should fail");

        assert!(err.to_string().contains("failed to execute"));
    }

    #[test]
    fn template_float_formatting_leaves_special_values_unsuffixed() {
        assert_eq!(format_float(f64::NAN), "NaN");
        assert_eq!(format_float(f64::INFINITY), "inf");
        assert_eq!(format_float(f64::NEG_INFINITY), "-inf");
        assert_eq!(format_float(2.0), "2.0");
    }
}
