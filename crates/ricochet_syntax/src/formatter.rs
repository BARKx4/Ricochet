use crate::ast::{ArgsDecl, ClassDecl, Expr, FunctionDecl, Item, MethodDecl, Module, SpannedExpr};
use crate::parse_module;
use crate::parser::ParseError;

const INDENT: &str = "  ";

pub fn format_source(source: &str) -> Result<String, ParseError> {
    let module = parse_module(source)?;
    let mut formatter = Formatter::default();
    formatter.format_module(&module);
    Ok(formatter.finish())
}

#[derive(Default)]
struct Formatter {
    output: String,
}

impl Formatter {
    fn finish(mut self) -> String {
        while self.output.ends_with("\n\n") {
            self.output.pop();
        }
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.output
    }

    fn format_module(&mut self, module: &Module) {
        for (index, item) in module.items.iter().enumerate() {
            if index > 0 {
                self.output.push('\n');
            }
            self.format_item(item, 0);
        }
    }

    fn format_item(&mut self, item: &Item, indent: usize) {
        match item {
            Item::Class(class) => {
                self.format_docs(&class.docs, indent);
                self.format_class(class, indent);
            }
            Item::Method(method) => {
                self.format_docs(&method.docs, indent);
                self.format_method(method, indent);
            }
            Item::Function(function) => {
                self.format_docs(&function.docs, indent);
                self.format_function(function, indent);
            }
            Item::Expr { expr, docs, .. } => {
                self.format_docs(docs, indent);
                self.format_statement(expr, indent);
            }
        }
    }

    fn format_docs(&mut self, docs: &[String], indent: usize) {
        for doc in docs {
            self.line(indent, &format!("(( {doc} ))"));
        }
    }

    fn format_class(&mut self, class: &ClassDecl, indent: usize) {
        self.line(
            indent,
            &format!("{} {} subclass", class.name, class.superclass),
        );
        for item in &class.body {
            self.format_item(item, indent + 1);
        }
        self.line(indent, "end");
    }

    fn format_method(&mut self, method: &MethodDecl, indent: usize) {
        let prefix = method
            .args
            .as_ref()
            .map(|args| format!("{} ", format_args(args)))
            .unwrap_or_default();
        self.line(indent, &format!("{prefix}{} method", method.name));
        self.format_body(&method.body, indent + 1);
        self.line(indent, "end");
    }

    fn format_function(&mut self, function: &FunctionDecl, indent: usize) {
        let prefix = function
            .args
            .as_ref()
            .map(|args| format!("{} ", format_args(args)))
            .unwrap_or_default();
        self.line(indent, &format!("{prefix}{} function", function.name));
        self.format_body(&function.body, indent + 1);
        self.line(indent, "end");
    }

    fn format_body(&mut self, body: &[SpannedExpr], indent: usize) {
        let mut inline = Vec::new();
        for expression in body {
            if is_inline_expr(&expression.expr) {
                inline.push(expression);
            } else {
                self.flush_inline(&inline, indent);
                inline.clear();
                self.format_statement(&expression.expr, indent);
            }
        }
        self.flush_inline(&inline, indent);
    }

    fn flush_inline(&mut self, expressions: &[&SpannedExpr], indent: usize) {
        if expressions.is_empty() {
            return;
        }

        let parts = expressions
            .iter()
            .map(|expr| format_expr_inline(&expr.expr))
            .collect::<Vec<_>>();
        self.line(indent, &parts.join(" "));
    }

    fn format_statement(&mut self, expr: &Expr, indent: usize) {
        match expr {
            Expr::Sequence(exprs) => self.format_sequence(exprs, indent),
            Expr::If {
                then_body,
                else_body,
            } => self.format_if(&[], then_body, else_body, indent),
            Expr::While { condition, body } => self.format_while(condition, body, indent),
            Expr::Block(body) => {
                self.line(indent, "[");
                self.format_body(body, indent + 1);
                self.line(indent, "]");
            }
            other => self.line(indent, &format_expr_inline(other)),
        }
    }

    fn format_sequence(&mut self, exprs: &[SpannedExpr], indent: usize) {
        if let Some((name, block, bang)) = block_declaration(exprs) {
            self.line(indent, &format!("{} [", format_expr_inline(&name.expr)));
            self.format_body(block, indent + 1);
            self.line(indent, &format!("] {}", format_expr_inline(&bang.expr)));
            return;
        }

        if let Some((prefix, then_body, else_body)) = split_if_sequence(exprs) {
            self.format_if(prefix, then_body, else_body, indent);
            return;
        }

        let parts = exprs
            .iter()
            .map(|expr| format_expr_inline(&expr.expr))
            .collect::<Vec<_>>();
        self.line(indent, &parts.join(" "));
    }

    fn format_if(
        &mut self,
        prefix: &[SpannedExpr],
        then_body: &[SpannedExpr],
        else_body: &[SpannedExpr],
        indent: usize,
    ) {
        let condition = prefix
            .iter()
            .map(|expr| format_expr_inline(&expr.expr))
            .collect::<Vec<_>>();
        let head = if condition.is_empty() {
            "if".to_string()
        } else {
            format!("{} if", condition.join(" "))
        };
        self.line(indent, &head);
        self.format_body(then_body, indent + 1);
        if !else_body.is_empty() {
            self.line(indent, "else");
            self.format_body(else_body, indent + 1);
        }
        self.line(indent, "end");
    }

    fn format_while(&mut self, condition: &[SpannedExpr], body: &[SpannedExpr], indent: usize) {
        let condition = condition
            .iter()
            .map(|expr| format_expr_inline(&expr.expr))
            .collect::<Vec<_>>();
        self.line(indent, &format!("{} while", condition.join(" ")));
        self.format_body(body, indent + 1);
        self.line(indent, "end");
    }

    fn line(&mut self, indent: usize, text: &str) {
        for _ in 0..indent {
            self.output.push_str(INDENT);
        }
        self.output.push_str(text);
        self.output.push('\n');
    }
}

fn is_inline_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Symbol(_)
            | Expr::BangWord(_)
            | Expr::DotWord(_)
            | Expr::Reference(_)
            | Expr::String(_)
            | Expr::Number(_)
            | Expr::Args(_)
    )
}

fn block_declaration(
    exprs: &[SpannedExpr],
) -> Option<(&SpannedExpr, &[SpannedExpr], &SpannedExpr)> {
    match exprs {
        [name, block, bang] if matches!(&block.expr, Expr::Block(_)) => {
            if let Expr::Block(body) = &block.expr {
                Some((name, body.as_slice(), bang))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn split_if_sequence(
    exprs: &[SpannedExpr],
) -> Option<(&[SpannedExpr], &[SpannedExpr], &[SpannedExpr])> {
    let (last, prefix) = exprs.split_last()?;
    if let Expr::If {
        then_body,
        else_body,
    } = &last.expr
    {
        Some((prefix, then_body.as_slice(), else_body.as_slice()))
    } else {
        None
    }
}

fn format_expr_inline(expr: &Expr) -> String {
    match expr {
        Expr::Symbol(word) | Expr::BangWord(word) | Expr::DotWord(word) => word.clone(),
        Expr::Reference(name) => format!("${name}"),
        Expr::String(value) => format!("\"{}\"", escape_string(value)),
        Expr::Number(value) => value.to_string(),
        Expr::Args(args) => format_args(args),
        Expr::Block(body) => {
            let parts = body
                .iter()
                .map(|expr| format_expr_inline(&expr.expr))
                .collect::<Vec<_>>();
            format!("[ {} ]", parts.join(" "))
        }
        Expr::Sequence(exprs) => exprs
            .iter()
            .map(|expr| format_expr_inline(&expr.expr))
            .collect::<Vec<_>>()
            .join(" "),
        Expr::If { .. } => "if".to_string(),
        Expr::While { .. } => "while".to_string(),
    }
}

fn format_args(args: &ArgsDecl) -> String {
    let mut parts = Vec::new();
    parts.extend(args.inputs.iter().cloned());
    if !args.outputs.is_empty() {
        parts.push("->".to_string());
        parts.extend(args.outputs.iter().cloned());
    }
    format!("( {} )", parts.join(" "))
}

fn escape_string(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            ch => vec![ch],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_class_block_methods_and_fields() {
        let source = r#"
User Model subclass
email field
"label" [ self .email get ] !method
end
"#;

        let formatted = format_source(source).expect("source should format");

        assert_eq!(
            formatted,
            "User Model subclass\n  email field\n  \"label\" [\n    self .email get\n  ] !method\nend\n"
        );
    }

    #[test]
    fn formats_postfix_if_else() {
        let formatted =
            format_source(r#"false if "yes" else "no" end"#).expect("source should format");

        assert_eq!(formatted, "false if\n  \"yes\"\nelse\n  \"no\"\nend\n");
    }

    #[test]
    fn formats_dollar_references() {
        let formatted = format_source("$ctx .params .id").expect("source should format");

        assert_eq!(formatted, "$ctx .params .id\n");
    }

    #[test]
    fn preserves_doc_comments() {
        let source = r#"
(( User docs ))
User Model subclass
(( Email docs ))
email field
end
"#;

        let formatted = format_source(source).expect("source should format");

        assert_eq!(
            formatted,
            "(( User docs ))\nUser Model subclass\n  (( Email docs ))\n  email field\nend\n"
        );
    }
}
