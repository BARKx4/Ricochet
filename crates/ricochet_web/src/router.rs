use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub controller: String,
    pub action: String,
}

pub fn parse_routes(source: &str) -> Result<Vec<Route>> {
    let mut routes = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let tokens = tokenize_route_line(line)
            .with_context(|| format!("invalid route on line {line_number}"))?;
        if tokens.len() != 5 || tokens[4] != "route" {
            bail!("unsupported route expression on line {line_number}: {line}");
        }

        routes.push(Route {
            method: tokens[0].clone(),
            path: tokens[1].clone(),
            controller: tokens[2].clone(),
            action: tokens[3].clone(),
        });
    }

    Ok(routes)
}

fn tokenize_route_line(line: &str) -> Result<Vec<String>> {
    let chars = line.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < chars.len() {
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= chars.len() {
            break;
        }

        if chars[pos] == '"' {
            pos += 1;
            let mut token = String::new();
            while pos < chars.len() && chars[pos] != '"' {
                token.push(chars[pos]);
                pos += 1;
            }
            if pos >= chars.len() {
                bail!("unterminated quoted route token");
            }
            pos += 1;
            tokens.push(token);
        } else {
            let mut token = String::new();
            while pos < chars.len() && !chars[pos].is_whitespace() {
                token.push(chars[pos]);
                pos += 1;
            }
            tokens.push(token);
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_parses_quoted_path_and_action_route() {
        let routes = parse_routes(r#"GET "/" HomeController "index" route"#)
            .expect("route should parse");

        assert_eq!(
            routes,
            vec![Route {
                method: "GET".to_string(),
                path: "/".to_string(),
                controller: "HomeController".to_string(),
                action: "index".to_string(),
            }]
        );
    }
}
