use std::{borrow::Cow, collections::BTreeMap};

use anyhow::{anyhow, bail, Context, Result};
use ricochet_compiler::compile_source;
use ricochet_syntax::{lex, Token, TokenKind};
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
    let nodes = parse_template(template)?;
    let mut context = TemplateContext::new(data);
    render_nodes(&nodes, &mut context, escape)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TemplateNode {
    Text(String),
    Expression(String),
    Script(String),
    If {
        condition: String,
        then_nodes: Vec<TemplateNode>,
        else_nodes: Vec<TemplateNode>,
    },
    Each {
        collection: String,
        item_name: String,
        body: Vec<TemplateNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StopDirective {
    Else,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Directive {
    If(String),
    Each {
        collection: String,
        item_name: String,
    },
    Script(String),
    Else,
    End,
}

struct TemplateContext {
    variables: BTreeMap<String, Value>,
}

impl TemplateContext {
    fn new(data: &BTreeMap<String, Value>) -> Self {
        Self {
            variables: data.clone(),
        }
    }
}

fn parse_template(template: &str) -> Result<Vec<TemplateNode>> {
    let mut pos = 0;
    let (nodes, stop) = parse_nodes(template, &mut pos, false)?;
    if let Some(stop) = stop {
        bail!("unexpected template directive {stop:?}");
    }
    Ok(nodes)
}

fn parse_nodes(
    template: &str,
    pos: &mut usize,
    allow_stop: bool,
) -> Result<(Vec<TemplateNode>, Option<StopDirective>)> {
    let mut nodes = Vec::new();

    while *pos < template.len() {
        let rest = &template[*pos..];
        let next_open = rest.find('{');
        let next_close = rest.find('}');

        match (next_open, next_close) {
            (None, None) => {
                nodes.push(TemplateNode::Text(rest.to_string()));
                *pos = template.len();
                break;
            }
            (None, Some(_)) => bail!("unmatched closing template brace"),
            (Some(open), Some(close)) if close < open => {
                bail!("unmatched closing template brace")
            }
            (Some(open), _) => {
                if open > 0 {
                    nodes.push(TemplateNode::Text(rest[..open].to_string()));
                }

                let open_index = *pos + open;
                ensure_text_context(template, open_index)?;
                if template[open_index..].starts_with("{%") {
                    let directive_start = open_index + 2;
                    let directive_rest = &template[directive_start..];
                    let directive_close = directive_rest
                        .find("%}")
                        .ok_or_else(|| anyhow!("unterminated template directive"))?;
                    let directive_end = directive_start + directive_close;
                    let directive =
                        parse_directive(template[directive_start..directive_end].trim())?;
                    *pos = directive_end + 2;

                    match directive {
                        Directive::If(condition) => {
                            let (then_nodes, stop) = parse_nodes(template, pos, true)?;
                            let else_nodes = match stop {
                                Some(StopDirective::Else) => {
                                    let (else_nodes, stop) = parse_nodes(template, pos, true)?;
                                    match stop {
                                        Some(StopDirective::End) => else_nodes,
                                        Some(StopDirective::Else) => {
                                            bail!("template if block has more than one else")
                                        }
                                        None => bail!("unterminated template if block"),
                                    }
                                }
                                Some(StopDirective::End) => Vec::new(),
                                None => bail!("unterminated template if block"),
                            };
                            nodes.push(TemplateNode::If {
                                condition,
                                then_nodes,
                                else_nodes,
                            });
                        }
                        Directive::Each {
                            collection,
                            item_name,
                        } => {
                            let (body, stop) = parse_nodes(template, pos, true)?;
                            match stop {
                                Some(StopDirective::End) => nodes.push(TemplateNode::Each {
                                    collection,
                                    item_name,
                                    body,
                                }),
                                Some(StopDirective::Else) => {
                                    bail!("template each block does not support else")
                                }
                                None => bail!("unterminated template each block"),
                            }
                        }
                        Directive::Script(source) => nodes.push(TemplateNode::Script(source)),
                        Directive::Else => {
                            if allow_stop {
                                return Ok((nodes, Some(StopDirective::Else)));
                            }
                            bail!("unexpected template else directive");
                        }
                        Directive::End => {
                            if allow_stop {
                                return Ok((nodes, Some(StopDirective::End)));
                            }
                            bail!("unexpected template end directive");
                        }
                    }
                } else {
                    let expression_start = open_index + 1;
                    let expression_rest = &template[expression_start..];
                    let expression_close = expression_rest
                        .find('}')
                        .ok_or_else(|| anyhow!("unterminated template expression"))?;
                    let expression_end = expression_start + expression_close;
                    let expression = template[expression_start..expression_end].trim();
                    if expression.is_empty() || expression.contains('{') {
                        bail!("unsupported template expression: {expression:?}");
                    }
                    nodes.push(TemplateNode::Expression(expression.to_string()));
                    *pos = expression_end + 1;
                }
            }
        }
    }

    Ok((nodes, None))
}

fn parse_directive(source: &str) -> Result<Directive> {
    if source.is_empty() || source.contains('{') {
        bail!("unsupported template directive: {source:?}");
    }
    if source == "else" {
        return Ok(Directive::Else);
    }
    if source == "end" {
        return Ok(Directive::End);
    }

    let tokens = directive_tokens(source)?;
    let Some(last) = tokens.last() else {
        bail!("unsupported template directive: {source:?}");
    };

    match &last.kind {
        TokenKind::Symbol(word) if word == "if" => {
            let condition = source[..last.span.start].trim();
            if condition.is_empty() {
                bail!("template if directive requires a condition before if");
            }
            Ok(Directive::If(condition.to_string()))
        }
        TokenKind::Symbol(word) if word == "do" => {
            let script = source[..last.span.start].trim();
            if script.is_empty() {
                bail!("template do directive requires Ricochet code before do");
            }
            Ok(Directive::Script(script.to_string()))
        }
        TokenKind::Symbol(word) if word == "each" => {
            if tokens.len() < 3 {
                bail!("template each directive requires collection expression and item name");
            }
            let item = &tokens[tokens.len() - 2];
            let item_name = match &item.kind {
                TokenKind::String(name) if valid_template_binding_name(name) => name.clone(),
                TokenKind::String(_) => {
                    bail!("template each item name must be a non-empty variable name")
                }
                _ => bail!("template each directive requires a string item name before each"),
            };
            let collection = source[..item.span.start].trim();
            if collection.is_empty() {
                bail!("template each directive requires a collection expression before item name");
            }
            Ok(Directive::Each {
                collection: collection.to_string(),
                item_name,
            })
        }
        _ => bail!("unsupported template directive: {source:?}"),
    }
}

fn directive_tokens(source: &str) -> Result<Vec<Token>> {
    let tokens =
        lex(source).map_err(|error| anyhow!("failed to lex template directive: {error}"))?;
    Ok(tokens
        .into_iter()
        .filter(|token| {
            !matches!(
                token.kind,
                TokenKind::Newline | TokenKind::DocComment(_) | TokenKind::Eof
            )
        })
        .collect())
}

fn valid_template_binding_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && !name.chars().any(char::is_control)
        && !name.chars().any(char::is_whitespace)
}

fn render_nodes(
    nodes: &[TemplateNode],
    context: &mut TemplateContext,
    escape: EscapeMode,
) -> Result<String> {
    let mut rendered = String::new();

    for node in nodes {
        match node {
            TemplateNode::Text(text) => rendered.push_str(text),
            TemplateNode::Expression(expression) => {
                let value = evaluate_expression(expression, context)?;
                rendered.push_str(&render_escaped_value(&value, escape)?);
            }
            TemplateNode::Script(script) => evaluate_script(script, context)?,
            TemplateNode::If {
                condition,
                then_nodes,
                else_nodes,
            } => {
                let condition = evaluate_expression(condition, context)?;
                let truthy = condition.truthy_for_condition()?;
                let branch = if truthy { then_nodes } else { else_nodes };
                rendered.push_str(&render_nodes(branch, context, escape)?);
            }
            TemplateNode::Each {
                collection,
                item_name,
                body,
            } => {
                let collection = evaluate_expression(collection, context)?;
                let items = template_each_items(&collection)?;
                let previous = context.variables.get(item_name).cloned();
                for item in items {
                    context.variables.insert(item_name.clone(), item);
                    rendered.push_str(&render_nodes(body, context, escape)?);
                }
                match previous {
                    Some(value) => {
                        context.variables.insert(item_name.clone(), value);
                    }
                    None => {
                        context.variables.remove(item_name);
                    }
                }
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

fn evaluate_expression(expression: &str, context: &mut TemplateContext) -> Result<Value> {
    let chunk = compile_source("<template>", expression)
        .with_context(|| format!("failed to compile template expression {expression:?}"))?;
    let mut vm = Vm::default();
    vm.set_instruction_limit(TEMPLATE_INSTRUCTION_LIMIT);
    for (name, value) in &context.variables {
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
    let value = value.clone();
    context.variables = vm.variables().clone();

    Ok(value)
}

fn evaluate_script(script: &str, context: &mut TemplateContext) -> Result<()> {
    let chunk = compile_source("<template>", script)
        .with_context(|| format!("failed to compile template script block {script:?}"))?;
    let mut vm = Vm::default();
    vm.set_instruction_limit(TEMPLATE_INSTRUCTION_LIMIT);
    for (name, value) in &context.variables {
        vm.set_variable(name.clone(), value.clone());
    }
    vm.run_chunk(&chunk)
        .with_context(|| format!("failed to execute template script block {script:?}"))?;
    if !vm.stack().is_empty() {
        bail!(
            "template script block {script:?} must leave no values, left {}",
            vm.stack().len()
        );
    }
    context.variables = vm.variables().clone();
    Ok(())
}

fn template_each_items(collection: &Value) -> Result<Vec<Value>> {
    match collection {
        Value::Array(values) => Ok(values.snapshot()),
        Value::List(values) => Ok(values.snapshot()),
        Value::Set(values) => Ok(values.snapshot()),
        Value::Map(values) => Ok(values
            .entries()
            .into_iter()
            .map(|(key, value)| {
                Value::Map(
                    BTreeMap::from([
                        ("key".to_string(), Value::String(key)),
                        ("value".to_string(), value),
                    ])
                    .into(),
                )
            })
            .collect()),
        value => bail!("template each expected array, list, set, or map; got {value:?}"),
    }
}

fn render_escaped_value(value: &Value, escape: EscapeMode) -> Result<String> {
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
    fn template_script_blocks_define_locals_for_later_interpolation() {
        let data = BTreeMap::new();

        let rendered = render_template(
            r#"{% "Hello <Ada>" "heading" var do %}<h1>{ heading get }</h1>"#,
            &data,
            EscapeMode::Html,
        )
        .expect("template script block should define local");

        assert_eq!(rendered, "<h1>Hello &lt;Ada&gt;</h1>");
    }

    #[test]
    fn template_if_else_blocks_render_selected_branch() {
        let data = BTreeMap::from([
            ("show".to_string(), Value::Bool(false)),
            ("title".to_string(), Value::String("Visible".to_string())),
        ]);

        let rendered = render_template(
            r#"{% show get if %}<p>{ title get }</p>{% else %}<p>Hidden</p>{% end %}"#,
            &data,
            EscapeMode::Html,
        )
        .expect("template if block should render");

        assert_eq!(rendered, "<p>Hidden</p>");
    }

    #[test]
    fn template_each_blocks_render_escaped_collection_items() {
        let data = BTreeMap::from([(
            "users".to_string(),
            Value::Array(
                vec![
                    Value::Map(
                        BTreeMap::from([(
                            "name".to_string(),
                            Value::String("Ada <Lovelace>".to_string()),
                        )])
                        .into(),
                    ),
                    Value::Map(
                        BTreeMap::from([("name".to_string(), Value::String("Grace".to_string()))])
                            .into(),
                    ),
                ]
                .into(),
            ),
        )]);

        let rendered = render_template(
            r#"<ul>{% users get "user" each %}<li>{ user get "name" at }</li>{% end %}</ul>"#,
            &data,
            EscapeMode::Html,
        )
        .expect("template each block should render");

        assert_eq!(
            rendered,
            "<ul><li>Ada &lt;Lovelace&gt;</li><li>Grace</li></ul>"
        );
    }

    #[test]
    fn template_blocks_can_nest_conditionals_inside_loops() {
        let data = BTreeMap::from([(
            "users".to_string(),
            Value::Array(
                vec![
                    Value::Map(
                        BTreeMap::from([
                            ("name".to_string(), Value::String("Ada".to_string())),
                            ("active".to_string(), Value::Bool(true)),
                        ])
                        .into(),
                    ),
                    Value::Map(
                        BTreeMap::from([
                            ("name".to_string(), Value::String("Grace".to_string())),
                            ("active".to_string(), Value::Bool(false)),
                        ])
                        .into(),
                    ),
                ]
                .into(),
            ),
        )]);

        let rendered = render_template(
            r#"{% users get "user" each %}{% user get "active" at if %}{ user get "name" at }{% end %}{% end %}"#,
            &data,
            EscapeMode::Html,
        )
        .expect("nested template blocks should render");

        assert_eq!(rendered, "Ada");
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
    fn template_script_blocks_require_empty_stack() {
        let data = BTreeMap::new();

        let err = render_template("{% 1 do %}", &data, EscapeMode::Html)
            .expect_err("script block with output should fail");

        assert!(err.to_string().contains("must leave no values"));
    }

    #[test]
    fn template_fails_on_unterminated_expression() {
        let data = BTreeMap::new();

        let err = render_template("<h1>{ title get</h1>", &data, EscapeMode::Html)
            .expect_err("unterminated expression should fail");

        assert!(err.to_string().contains("unterminated"));
    }

    #[test]
    fn template_fails_on_unterminated_control_block() {
        let data = BTreeMap::from([("show".to_string(), Value::Bool(true))]);

        let err = render_template("{% show get if %}<p>Visible</p>", &data, EscapeMode::Html)
            .expect_err("unterminated if should fail");

        assert!(err.to_string().contains("unterminated template if block"));
    }

    #[test]
    fn template_rejects_unexpected_end_directive() {
        let data = BTreeMap::new();

        let err = render_template("{% end %}", &data, EscapeMode::Html)
            .expect_err("unexpected end should fail");

        assert!(err
            .to_string()
            .contains("unexpected template end directive"));
    }

    #[test]
    fn template_rejects_result_conditions_like_runtime_if() {
        let data = BTreeMap::new();

        let err = render_template(
            "{% \"TemplateError\" \"bad\" fail if %}<p>bad</p>{% end %}",
            &data,
            EscapeMode::Html,
        )
        .expect_err("result condition should fail");

        assert!(err.to_string().contains("explicit ok? check"));
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
