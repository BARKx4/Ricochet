use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ricochet_bytecode::Chunk;
use ricochet_compiler::{compile_file_with_imports, compile_source, CompileError};
use ricochet_syntax::{
    format_source, parse_module, ArgsDecl, Expr, Item as SyntaxItem, LexError, Module, ParseError,
    SpannedExpr, TokenKind,
};
use ricochet_vm::{DebugAction, DebugEvent, DebugPauseReason, Vm};
use toml_edit::{value, DocumentMut, Item, Table};

const DEFAULT_BUILD_SOURCE: &str = "main.rco";
const BUILD_OUTPUT: &str = "build/app.rcob";
const EMBEDDED_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_APP_V1\0";

#[derive(Debug, Parser)]
#[command(name = "rco")]
#[command(about = "Ricochet language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    New {
        path: String,
    },
    Check {
        path: Option<String>,
    },
    Repl {
        #[arg(long)]
        debug: bool,
    },
    Run {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        step: bool,
        #[arg(long = "breakpoint", value_name = "LINE")]
        breakpoints: Vec<usize>,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    RunBytecode {
        #[arg(long)]
        debug: bool,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Build {
        path: Option<String>,
    },
    Package {
        path: Option<String>,
        #[arg(short, long)]
        output: PathBuf,
    },
    Add {
        source: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        no_fetch: bool,
    },
    Install,
    Doc {
        path: Option<String>,
    },
    Fmt {
        #[arg(long)]
        check: bool,
        path: Option<String>,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 3000)]
        port: u16,
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        watch: bool,
    },
    Routes {
        path: Option<String>,
    },
    Test {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        filter: Option<String>,
        path: Option<String>,
    },
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub async fn run_cli() -> Result<()> {
    if let Some(chunk) = embedded_chunk_from_current_exe()? {
        run_chunk_cli(
            &chunk,
            false,
            false,
            &[],
            None,
            std::env::args().skip(1).collect(),
        )?;
        return Ok(());
    }

    let cli = Cli::parse();
    match cli.command {
        Command::New { path } => new_project(Path::new(&path))?,
        Command::Check { path } => check(path.as_deref().unwrap_or("."))?,
        Command::Repl { debug } => {
            let stdin = io::stdin();
            let interactive = stdin.is_terminal();
            let stdout = io::stdout();
            run_repl(stdin.lock(), stdout.lock(), debug, interactive)?;
        }
        Command::Run {
            debug,
            step,
            breakpoints,
            path,
            args,
        } => run_file(&path, debug, step, &breakpoints, args)?,
        Command::RunBytecode { debug, path, args } => run_bytecode(&path, debug, args)?,
        Command::Build { path } => build(path.as_deref().unwrap_or(DEFAULT_BUILD_SOURCE))?,
        Command::Package { path, output } => {
            package(path.as_deref().unwrap_or(DEFAULT_BUILD_SOURCE), &output)?
        }
        Command::Add {
            source,
            name,
            no_fetch,
        } => add_dependency(&source, name.as_deref(), no_fetch)?,
        Command::Install => install_dependencies()?,
        Command::Doc { path } => doc_path(path.as_deref().unwrap_or("."))?,
        Command::Fmt { check, path } => format_path(path.as_deref().unwrap_or("."), check)?,
        Command::Serve {
            host,
            port,
            debug,
            watch,
        } => {
            ricochet_web::serve_current_dir(ricochet_web::ServeOptions {
                host,
                port,
                debug,
                watch,
            })
            .await?
        }
        Command::Routes { path } => routes(path.as_deref().unwrap_or("."))?,
        Command::Test {
            debug,
            filter,
            path,
        } => {
            run_tests(path.as_deref().unwrap_or("tests"), debug, filter.as_deref())?;
        }
    }
    Ok(())
}

fn run_repl<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    debug: bool,
    interactive: bool,
) -> Result<()> {
    let mut vm = Vm::default();
    vm.enable_cli_capabilities();
    if debug {
        vm.enable_debug();
        vm.set_debug_sink(print_debug_event);
    }

    let mut source = String::new();
    let mut line = String::new();
    let mut output_cursor = 0usize;

    loop {
        if interactive {
            let prompt = if source.is_empty() { "rco> " } else { "...> " };
            write!(output, "{prompt}")?;
            output.flush()?;
        }

        line.clear();
        let bytes_read = input.read_line(&mut line)?;
        if bytes_read == 0 {
            if source.trim().is_empty() {
                return Ok(());
            }

            return match compile_source("<repl>", &source) {
                Err(error) if is_incomplete_compile_error(&error) => {
                    bail!("incomplete Ricochet input: {error}")
                }
                Err(error) => Err(error.into()),
                Ok(chunk) => {
                    vm.run_chunk(&chunk)?;
                    write_repl_result(&vm, &mut output, &mut output_cursor)?;
                    Ok(())
                }
            };
        }

        source.push_str(&line);
        if source.trim().is_empty() {
            source.clear();
            continue;
        }

        match compile_source("<repl>", &source) {
            Ok(chunk) => {
                match vm.run_chunk(&chunk) {
                    Ok(()) => write_repl_result(&vm, &mut output, &mut output_cursor)?,
                    Err(error) => writeln!(output, "ERROR {error}")?,
                }
                source.clear();
            }
            Err(error) if is_incomplete_compile_error(&error) => {}
            Err(error) => {
                writeln!(output, "ERROR {error}")?;
                source.clear();
            }
        }
    }
}

fn write_repl_result<W: Write>(vm: &Vm, output: &mut W, output_cursor: &mut usize) -> Result<()> {
    write!(output, "{}", &vm.stdout()[*output_cursor..])?;
    *output_cursor = vm.stdout().len();
    writeln!(output, "{:?}", vm.stack())?;
    output.flush()?;
    Ok(())
}

fn is_incomplete_compile_error(error: &CompileError) -> bool {
    matches!(
        error,
        CompileError::Parse(ParseError::Expected {
            found: TokenKind::Eof,
            ..
        }) | CompileError::Parse(ParseError::Unexpected {
            found: TokenKind::Eof,
            ..
        }) | CompileError::Parse(ParseError::Lex(
            LexError::UnterminatedString(_) | LexError::UnterminatedComment(_),
        ))
    )
}

fn check(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_file() {
        check_source_file(path)?;
        println!("checked {}", path.display());
        return Ok(());
    }

    if !path.is_dir() {
        bail!("check path does not exist: {}", path.display());
    }

    if path.join("ricochet.toml").is_file() {
        let _app = ricochet_web::server::build_app_from_dir(path)
            .with_context(|| format!("failed to check MVC app {}", path.display()))?;
        println!("checked {}", path.display());
        return Ok(());
    }

    let mut files = Vec::new();
    collect_rco_files(path, &mut files)?;
    files.sort();
    for file in &files {
        check_source_file(file)?;
    }

    println!(
        "checked {} Ricochet files in {}",
        files.len(),
        path.display()
    );
    Ok(())
}

fn check_source_file(path: &Path) -> Result<()> {
    compile_source_file(path).with_context(|| format!("failed to compile {}", path.display()))?;
    Ok(())
}

fn routes(path: &str) -> Result<()> {
    let path = Path::new(path);
    if !path.is_dir() {
        bail!(
            "routes path does not exist or is not a directory: {}",
            path.display()
        );
    }

    for route in ricochet_web::routes_from_dir(path)? {
        println!(
            "{} {} {}#{}",
            route.method, route.path, route.controller, route.action
        );
    }

    Ok(())
}

fn doc_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if !path.exists() {
        bail!("doc path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    if path.is_file() {
        files.push(path.to_path_buf());
    } else {
        collect_rco_files(path, &mut files)?;
        files.sort();
    }

    let mut output = String::new();
    writeln!(&mut output, "# Ricochet Documentation")?;
    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let module =
            parse_module(&source).with_context(|| format!("failed to parse {}", file.display()))?;
        write_module_docs(&mut output, &file, &module)?;
    }
    print!("{output}");
    Ok(())
}

fn write_module_docs(output: &mut String, file: &Path, module: &Module) -> Result<()> {
    writeln!(output)?;
    writeln!(output, "## File `{}`", file.display())?;
    for item in &module.items {
        write_item_docs(output, item, 0)?;
    }
    Ok(())
}

fn write_item_docs(output: &mut String, item: &SyntaxItem, indent: usize) -> Result<()> {
    match item {
        SyntaxItem::Class(class) => {
            writeln!(output)?;
            writeln!(output, "{}## Class `{}`", doc_indent(indent), class.name)?;
            writeln!(
                output,
                "{}Extends `{}`.",
                doc_indent(indent),
                class.superclass
            )?;
            write_docs(output, &class.docs, indent)?;
            for item in &class.body {
                write_item_docs(output, item, indent + 1)?;
            }
        }
        SyntaxItem::Function(function) => {
            writeln!(output)?;
            writeln!(
                output,
                "{}## Function `{}`{}",
                doc_indent(indent),
                function.name,
                doc_args(function.args.as_ref())
            )?;
            write_docs(output, &function.docs, indent)?;
        }
        SyntaxItem::Method(method) => {
            writeln!(
                output,
                "{}- Method: `{}`{}",
                doc_indent(indent),
                method.name,
                doc_args(method.args.as_ref())
            )?;
            write_docs(output, &method.docs, indent + 1)?;
        }
        SyntaxItem::Expr { expr, docs, .. } => {
            if let Some((kind, name)) = documented_expr_declaration(expr) {
                writeln!(output, "{}- {kind}: `{name}`", doc_indent(indent))?;
                write_docs(output, docs, indent + 1)?;
            }
        }
    }
    Ok(())
}

fn documented_expr_declaration(expr: &Expr) -> Option<(&'static str, &str)> {
    let Expr::Sequence(exprs) = expr else {
        return None;
    };
    let [name, declaration] = exprs.as_slice() else {
        return None;
    };
    let name = declaration_name(name)?;
    match &declaration.expr {
        Expr::Symbol(word) if word == "field" => Some(("Field", name)),
        Expr::Symbol(word) if word == "table" => Some(("Table", name)),
        _ => None,
    }
}

fn declaration_name(expression: &SpannedExpr) -> Option<&str> {
    match &expression.expr {
        Expr::Symbol(name) | Expr::String(name) => Some(name),
        _ => None,
    }
}

fn write_docs(output: &mut String, docs: &[String], indent: usize) -> Result<()> {
    for doc in docs {
        writeln!(output, "{}{}", doc_indent(indent), doc)?;
    }
    Ok(())
}

fn doc_args(args: Option<&ArgsDecl>) -> String {
    let Some(args) = args else {
        return String::new();
    };

    let inputs = if args.inputs.is_empty() {
        String::new()
    } else {
        args.inputs.join(" ")
    };
    let outputs = if args.outputs.is_empty() {
        String::new()
    } else {
        format!(" -> {}", args.outputs.join(" "))
    };
    format!(" `({inputs}{outputs})`")
}

fn doc_indent(indent: usize) -> String {
    "  ".repeat(indent)
}

fn new_project(path: &Path) -> Result<()> {
    ensure_project_path_is_ready(path)?;

    fs::create_dir_all(path.join("app").join("Controllers"))
        .with_context(|| format!("failed to create app/Controllers in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Models"))
        .with_context(|| format!("failed to create app/Models in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Views").join("home"))
        .with_context(|| format!("failed to create app/Views/home in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Views").join("users"))
        .with_context(|| format!("failed to create app/Views/users in {}", path.display()))?;
    fs::create_dir_all(path.join("config"))
        .with_context(|| format!("failed to create config in {}", path.display()))?;
    fs::create_dir_all(path.join("tests"))
        .with_context(|| format!("failed to create tests in {}", path.display()))?;

    write_project_file(
        path.join("ricochet.toml"),
        &manifest_source(&project_name(path)),
    )?;
    write_project_file(
        path.join("config").join("routes.rco"),
        r#"GET "/" HomeController "index" route
GET "/users" UserController "index" route
"#,
    )?;
    write_project_file(
        path.join("app")
            .join("Controllers")
            .join("HomeController.rco"),
        r#"HomeController Controller subclass
  "index" [
    "Hello Ricochet" title var
    ctx get
    "home/index" swap view
  ] !method
end
"#,
    )?;
    write_project_file(
        path.join("app").join("Models").join("User.rco"),
        r#"User Model subclass
  email field
  name field

  "displayName" [
    self .name get nil? if
      self .email get
    else
      self .name get
    end
  ] !method
end
"#,
    )?;
    write_project_file(
        path.join("app")
            .join("Controllers")
            .join("UserController.rco"),
        r#"UserController Controller subclass
  "index" [
    users array
    User new
    "ada@example.com" swap .email set
    "Ada Lovelace" swap .name set
    users get .push! drop
    users get .count userCount var
    "Users" title var
    ctx get
    "users/index" swap view
  ] !method
end
"#,
    )?;
    write_project_file(
        path.join("app")
            .join("Views")
            .join("home")
            .join("index.html"),
        "<h1>{ title get }</h1>\n",
    )?;
    write_project_file(
        path.join("app")
            .join("Views")
            .join("users")
            .join("index.html"),
        "<h1>{ title get }</h1>\n<p>{ userCount get } users ready.</p>\n",
    )?;
    write_project_file(
        path.join("tests").join("ApplicationSmokeTest.rco"),
        r#"ApplicationSmokeTest TestCase subclass
  "testUserDisplayNameFallsBackToEmail" [
    User new
    "ada@example.com" swap .email set
    .displayName
    "ada@example.com" assert-equals
  ] !method

  "testCollectionsCanHoldModels" [
    users array
    User new
    "grace@example.com" swap .email set
    users get .push! drop
    users get .count
    1 assert-equals
  ] !method
end
"#,
    )?;

    println!("created {}", path.display());
    Ok(())
}

fn ensure_project_path_is_ready(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!(
                "project path already exists and is not a directory: {}",
                path.display()
            );
        }

        if fs::read_dir(path)
            .with_context(|| format!("failed to read {}", path.display()))?
            .next()
            .transpose()
            .with_context(|| format!("failed to read entry in {}", path.display()))?
            .is_some()
        {
            bail!(
                "project path already exists and is not empty: {}",
                path.display()
            );
        }
    }

    Ok(())
}

fn write_project_file(path: PathBuf, contents: &str) -> Result<()> {
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn manifest_source(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#
    )
}

fn project_name(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ricochet_app");
    let sanitized = name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => ch,
            _ => '_',
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "ricochet_app".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug)]
struct DependencySpec {
    name: String,
    path: String,
    source: String,
    git: Option<String>,
    rev: Option<String>,
    display_source: String,
}

#[derive(Debug)]
enum DependencySource {
    Local {
        path: PathBuf,
    },
    GitHub {
        owner: String,
        repo: String,
        rev: Option<String>,
    },
}

fn add_dependency(source: &str, name: Option<&str>, no_fetch: bool) -> Result<()> {
    let manifest_path = find_project_manifest_for_current_dir("add")?;
    let project_root = manifest_path
        .parent()
        .expect("project manifest should have a parent");
    let dependency_source = parse_dependency_source(source)?;
    let spec = dependency_spec(project_root, source, dependency_source, name)?;

    if spec.git.is_some() && !no_fetch {
        fetch_git_dependency(project_root, &spec)?;
    }

    write_dependency_manifest(&manifest_path, &spec)?;
    write_lockfile(&project_root.join("ricochet.lock"), &spec)?;

    if spec.git.is_some() && no_fetch {
        println!(
            "added {} from {} (fetch skipped)",
            spec.name, spec.display_source
        );
    } else {
        println!("added {} from {}", spec.name, spec.display_source);
    }
    Ok(())
}

fn install_dependencies() -> Result<()> {
    let manifest_path = find_project_manifest_for_current_dir("install")?;
    let project_root = manifest_path
        .parent()
        .expect("project manifest should have a parent");
    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let Some(dependencies) = doc.get("dependencies").and_then(Item::as_table) else {
        println!("no dependencies to install");
        return Ok(());
    };

    let lock_path = project_root.join("ricochet.lock");
    let mut installed = 0usize;
    for (name, item) in dependencies.iter() {
        validate_package_name(name)?;
        let table = item.as_table().with_context(|| {
            format!(
                "dependency {name} in {} must be a table",
                manifest_path.display()
            )
        })?;
        let path = table
            .get("path")
            .and_then(Item::as_str)
            .with_context(|| {
                format!(
                    "dependency {name} in {} must include a string path",
                    manifest_path.display()
                )
            })?
            .to_string();
        let git = table.get("git").and_then(Item::as_str).map(str::to_string);
        let rev = table.get("rev").and_then(Item::as_str).map(str::to_string);
        let display_source = git.clone().unwrap_or_else(|| path.clone());
        let spec = DependencySpec {
            name: name.to_string(),
            source: git
                .as_ref()
                .map(|git| format!("git+{git}"))
                .unwrap_or_else(|| format!("path+{path}")),
            path: path.clone(),
            git,
            rev,
            display_source,
        };

        if spec.git.is_some() {
            let package_dir = project_root.join(&spec.path);
            if !package_dir.is_dir() {
                fetch_git_dependency(project_root, &spec)?;
            }
        } else {
            let dependency_dir = PathBuf::from(&spec.path);
            let dependency_dir = if dependency_dir.is_absolute() {
                dependency_dir
            } else {
                project_root.join(dependency_dir)
            };
            if !dependency_dir.is_dir() {
                bail!(
                    "local Ricochet dependency {name} is not a directory: {}",
                    dependency_dir.display()
                );
            }
        }

        write_lockfile(&lock_path, &spec)?;
        println!("installed {} from {}", spec.name, spec.display_source);
        installed += 1;
    }

    if installed == 0 {
        println!("no dependencies to install");
    }
    Ok(())
}

fn find_project_manifest_for_current_dir(command: &str) -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to determine current directory")?;
    for ancestor in current_dir.ancestors() {
        let manifest_path = ancestor.join("ricochet.toml");
        if manifest_path.is_file() {
            return Ok(manifest_path);
        }
    }

    bail!("rco {command} must be run inside a Ricochet project with ricochet.toml");
}

fn parse_dependency_source(source: &str) -> Result<DependencySource> {
    if let Some(rest) = source.strip_prefix("github:") {
        let (repository, rev) = rest
            .split_once('@')
            .map(|(repository, rev)| (repository, Some(rev.to_string())))
            .unwrap_or((rest, None));
        let (owner, repo) = repository.split_once('/').with_context(|| {
            format!("invalid GitHub dependency {source:?}; expected github:owner/repo@ref")
        })?;
        if owner.is_empty() || repo.is_empty() || repo.contains('/') {
            bail!("invalid GitHub dependency {source:?}; expected github:owner/repo@ref");
        }
        return Ok(DependencySource::GitHub {
            owner: owner.to_string(),
            repo: repo.to_string(),
            rev,
        });
    }

    Ok(DependencySource::Local {
        path: PathBuf::from(source),
    })
}

fn dependency_spec(
    project_root: &Path,
    original_source: &str,
    source: DependencySource,
    name_override: Option<&str>,
) -> Result<DependencySpec> {
    match source {
        DependencySource::Local { path } => {
            let current_dir =
                std::env::current_dir().context("failed to determine current directory")?;
            let absolute_path = if path.is_absolute() {
                path
            } else {
                current_dir.join(path)
            };
            if !absolute_path.is_dir() {
                bail!(
                    "local Ricochet dependency is not a directory: {}",
                    absolute_path.display()
                );
            }

            let name = match name_override {
                Some(name) => name.to_string(),
                None => read_package_name(&absolute_path)?
                    .unwrap_or_else(|| directory_package_name(&absolute_path)),
            };
            validate_package_name(&name)?;
            let path = dependency_path_value(project_root, &absolute_path, original_source)?;

            Ok(DependencySpec {
                name,
                path: path.clone(),
                source: format!("path+{path}"),
                git: None,
                rev: None,
                display_source: path,
            })
        }
        DependencySource::GitHub { owner, repo, rev } => {
            let name = name_override.unwrap_or(&repo).to_string();
            validate_package_name(&name)?;
            let git = format!("https://github.com/{owner}/{repo}.git");
            let path = format!(".ricochet/packages/{name}");

            Ok(DependencySpec {
                name,
                path,
                source: format!("git+{git}"),
                git: Some(git),
                rev,
                display_source: original_source.to_string(),
            })
        }
    }
}

fn read_package_name(path: &Path) -> Result<Option<String>> {
    let manifest_path = path.join("ricochet.toml");
    if !manifest_path.is_file() {
        return Ok(None);
    }

    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    Ok(doc["package"]["name"].as_str().map(str::to_string))
}

fn directory_package_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package")
        .to_string()
}

fn validate_package_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        bail!("invalid Ricochet package name {name:?}; use letters, numbers, _ or -");
    }

    Ok(())
}

fn dependency_path_value(
    project_root: &Path,
    absolute_path: &Path,
    fallback: &str,
) -> Result<String> {
    let canonical_root = fs::canonicalize(project_root)
        .with_context(|| format!("failed to resolve {}", project_root.display()))?;
    let canonical_path = fs::canonicalize(absolute_path)
        .with_context(|| format!("failed to resolve {}", absolute_path.display()))?;
    if let Ok(relative_path) = canonical_path.strip_prefix(&canonical_root) {
        let relative = path_to_slash(relative_path);
        if relative.is_empty() {
            bail!("local dependency cannot point at the project root");
        }
        return Ok(format!("./{relative}"));
    }

    Ok(fallback.replace('\\', "/"))
}

fn path_to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn fetch_git_dependency(project_root: &Path, spec: &DependencySpec) -> Result<()> {
    let git = spec
        .git
        .as_deref()
        .expect("fetch_git_dependency only handles git dependencies");
    let package_dir = project_root.join(&spec.path);
    if package_dir.exists() {
        bail!(
            "package cache already exists: {}; remove it or choose a different --name",
            package_dir.display()
        );
    }

    if let Some(parent) = package_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut command = std::process::Command::new("git");
    command.arg("clone").arg("--depth").arg("1");
    if let Some(rev) = spec.rev.as_deref() {
        command.arg("--branch").arg(rev);
    }
    command.arg(git).arg(&package_dir);

    let output = command
        .output()
        .with_context(|| format!("failed to launch git to fetch {}", spec.display_source))?;
    if !output.status.success() {
        bail!(
            "failed to fetch Ricochet package {}\nstdout:\n{}\nstderr:\n{}",
            spec.display_source,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn write_dependency_manifest(manifest_path: &Path, spec: &DependencySpec) -> Result<()> {
    let source = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let mut doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let dependencies = ensure_table(doc.as_table_mut(), "dependencies", manifest_path)?;
    let mut dependency = Table::new();
    dependency["path"] = value(spec.path.clone());
    if let Some(git) = &spec.git {
        dependency["git"] = value(git.clone());
    }
    if let Some(rev) = &spec.rev {
        dependency["rev"] = value(rev.clone());
    }
    dependencies.insert(&spec.name, Item::Table(dependency));

    fs::write(manifest_path, doc.to_string())
        .with_context(|| format!("failed to write {}", manifest_path.display()))
}

fn write_lockfile(lock_path: &Path, spec: &DependencySpec) -> Result<()> {
    let source = if lock_path.is_file() {
        fs::read_to_string(lock_path)
            .with_context(|| format!("failed to read {}", lock_path.display()))?
    } else {
        String::new()
    };
    let mut doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", lock_path.display()))?;

    let packages = ensure_table(doc.as_table_mut(), "package", lock_path)?;
    let mut package = Table::new();
    package["source"] = value(spec.source.clone());
    package["path"] = value(spec.path.clone());
    if let Some(git) = &spec.git {
        package["git"] = value(git.clone());
    }
    if let Some(rev) = &spec.rev {
        package["rev"] = value(rev.clone());
    }
    packages.insert(&spec.name, Item::Table(package));

    fs::write(lock_path, doc.to_string())
        .with_context(|| format!("failed to write {}", lock_path.display()))
}

fn ensure_table<'a>(root: &'a mut Table, key: &str, path: &Path) -> Result<&'a mut Table> {
    if !root.contains_key(key) {
        root.insert(key, Item::Table(Table::new()));
    }

    root.get_mut(key)
        .and_then(Item::as_table_mut)
        .with_context(|| format!("{key} in {} must be a table", path.display()))
}

fn run_file(
    path: &str,
    debug: bool,
    step: bool,
    breakpoints: &[usize],
    args: Vec<String>,
) -> Result<()> {
    let chunk = compile_source_file(Path::new(path))?;
    run_chunk_cli(&chunk, debug, step, breakpoints, Some(&chunk.file), args)
}

fn run_bytecode(path: &str, debug: bool, args: Vec<String>) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {path}"))?;
    let chunk = Chunk::from_bytes(&bytes).with_context(|| format!("failed to decode {path}"))?;
    run_chunk_cli(&chunk, debug, false, &[], None, args)
}

fn run_chunk_cli(
    chunk: &Chunk,
    debug: bool,
    step: bool,
    breakpoints: &[usize],
    breakpoint_file: Option<&str>,
    args: Vec<String>,
) -> Result<()> {
    let mut vm = cli_vm(args);
    let debugger_enabled = debug || step || !breakpoints.is_empty();
    if debugger_enabled {
        vm.enable_debug();
        vm.set_debug_sink(print_debug_event);
    }
    if step {
        vm.enable_step_debugging();
    }
    for &line in breakpoints {
        if line == 0 {
            bail!("breakpoint lines are 1-based");
        }
        let file = breakpoint_file.unwrap_or(&chunk.file);
        vm.add_line_breakpoint(file.to_string(), line);
    }
    if step || !breakpoints.is_empty() {
        vm.set_debug_controller(|_| read_terminal_debug_action());
    }

    let result = vm.run_chunk(chunk);
    print!("{}", vm.stdout());
    eprint!("{}", vm.stderr());
    if let Err(ricochet_vm::VmError::ExitRequested { code }) = result {
        std::process::exit(code);
    }
    result?;

    println!("{:?}", vm.stack());

    Ok(())
}

fn cli_vm(args: Vec<String>) -> Vm {
    let mut vm = Vm::default();
    vm.enable_cli_capabilities();
    vm.set_program_args(args);
    vm.set_input_reader(|| {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|error| error.to_string())
            .map(|read| (read > 0).then_some(line))
    });
    vm
}

fn read_terminal_debug_action() -> DebugAction {
    loop {
        print!("debug> ");
        if io::stdout().flush().is_err() {
            return DebugAction::Abort;
        }

        let mut command = String::new();
        match io::stdin().read_line(&mut command) {
            Ok(0) | Err(_) => return DebugAction::Abort,
            Ok(_) => match command.trim().to_ascii_lowercase().as_str() {
                "" | "s" | "step" => return DebugAction::Step,
                "c" | "continue" => return DebugAction::Continue,
                "a" | "abort" | "q" | "quit" => return DebugAction::Abort,
                _ => println!("commands: step, continue, abort"),
            },
        }
    }
}

fn run_tests(path: &str, debug: bool, filter: Option<&str>) -> Result<()> {
    let path = Path::new(path);
    let project_root = find_project_root_for_tests(path);
    let files = collect_test_files_for_path(path, project_root.as_deref())?;
    let mut total = 0usize;
    let mut failed = 0usize;

    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        if filter.is_some_and(|filter| !test_file_may_match_filter(&source, filter)) {
            continue;
        }
        let chunk = compile_source_file(&file)?;
        let mut vm = Vm::default();
        vm.enable_cli_capabilities();
        if debug {
            vm.enable_debug();
            vm.set_debug_sink(print_debug_event);
        }

        if let Some(project_root) = project_root.as_deref() {
            load_app_sources(&mut vm, project_root)?;
        }

        vm.run_chunk(&chunk)
            .with_context(|| format!("failed to load test file {}", file.display()))?;

        let tests = vm.test_methods();
        for (class_name, method_name) in tests {
            let test_name = format!("{class_name}.{method_name}");
            if let Some(filter) = filter {
                if !test_name.contains(filter) {
                    continue;
                }
            }

            total += 1;
            let instance = vm
                .new_instance(&class_name)
                .with_context(|| format!("failed to instantiate test case {class_name}"))?;
            match vm.call_method_value(instance, &method_name) {
                Ok(_) => println!("PASS {test_name}"),
                Err(error) => {
                    failed += 1;
                    println!("FAIL {test_name}: {error}");
                }
            }
        }
    }

    println!("{total} tests, {failed} failed");
    if failed > 0 {
        bail!(
            "{} Ricochet test{} failed",
            failed,
            if failed == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

fn test_file_may_match_filter(source: &str, filter: &str) -> bool {
    source.contains(filter)
        || filter.split_once('.').is_some_and(|(class, method)| {
            !class.is_empty()
                && source.contains(class)
                && !method.is_empty()
                && source.contains(method)
        })
}

fn find_project_root_for_tests(path: &Path) -> Option<PathBuf> {
    let canonical = fs::canonicalize(path).ok()?;
    let start: &Path = if canonical.is_file() {
        canonical.parent()?
    } else {
        &canonical
    };

    start
        .ancestors()
        .find(|ancestor| ancestor.join("ricochet.toml").is_file())
        .map(Path::to_path_buf)
}

fn collect_test_files_for_path(path: &Path, project_root: Option<&Path>) -> Result<Vec<PathBuf>> {
    if let Some(project_root) = project_root {
        if paths_point_to_same_location(path, project_root) {
            let tests_path = project_root.join("tests");
            if !tests_path.exists() {
                return Ok(Vec::new());
            }
            return collect_test_files(&tests_path);
        }
    }

    collect_test_files(path)
}

fn paths_point_to_same_location(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn load_app_sources(vm: &mut Vm, project_root: &Path) -> Result<()> {
    for file in collect_app_source_files(project_root)? {
        let chunk = compile_source_file(&file)
            .with_context(|| format!("failed to compile app source {}", file.display()))?;
        vm.run_chunk(&chunk)
            .with_context(|| format!("failed to load app source {}", file.display()))?;
    }

    Ok(())
}

fn collect_app_source_files(project_root: &Path) -> Result<Vec<PathBuf>> {
    let app_path = project_root.join("app");
    if !app_path.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    for directory in ["Models", "Services", "Controllers"] {
        let source_path = app_path.join(directory);
        if source_path.is_dir() {
            push_unique_rco_files(&source_path, &mut seen, &mut files)?;
        }
    }

    push_unique_rco_files(&app_path, &mut seen, &mut files)?;
    Ok(files)
}

fn push_unique_rco_files(
    path: &Path,
    seen: &mut BTreeSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let mut source_files = Vec::new();
    collect_rco_files(path, &mut source_files)?;
    source_files.sort();
    for file in source_files {
        if seen.insert(file.clone()) {
            files.push(file);
        }
    }

    Ok(())
}

fn print_debug_event(event: &DebugEvent) {
    match event {
        DebugEvent::Paused(pause) => {
            let reason = match pause.reason {
                DebugPauseReason::Step => "step",
                DebugPauseReason::Breakpoint => "breakpoint",
            };
            println!(
                "PAUSE {reason} {} [{}] {}",
                pause.source, pause.frame, pause.opcode
            );
            println!("  stack:  {:?}", pause.stack);
        }
        DebugEvent::Instruction {
            frame,
            source,
            opcode,
            stack_before,
            stack_after,
        } => {
            println!("TRACE {source} [{frame}] {opcode}");
            println!("  before: {stack_before:?}");
            println!("  after:  {stack_after:?}");
        }
        DebugEvent::Fault {
            frame,
            message,
            stack,
        } => {
            println!("FAULT [{frame}] {message}");
            println!("  stack:  {stack:?}");
        }
    }
}

fn build(path: &str) -> Result<()> {
    let chunk = compile_source_file(Path::new(path))?;
    let output = Path::new(BUILD_OUTPUT);
    let parent = output
        .parent()
        .expect("build output path should include parent directory");

    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(output, chunk.to_bytes()?)
        .with_context(|| format!("failed to write {}", output.display()))?;

    println!("built {}", output.display());

    Ok(())
}

fn package(path: &str, output: &Path) -> Result<()> {
    if output.is_dir() {
        bail!("package output is a directory: {}", output.display());
    }

    let chunk = compile_source_file(Path::new(path))?;
    let bytes = chunk.to_bytes()?;
    let current_exe =
        std::env::current_exe().context("failed to locate current Ricochet executable")?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&current_exe, output).with_context(|| {
        format!(
            "failed to copy launcher {} to {}",
            current_exe.display(),
            output.display()
        )
    })?;
    append_embedded_chunk(output, &bytes)?;

    println!("packaged {}", output.display());
    Ok(())
}

fn append_embedded_chunk(path: &Path, chunk_bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {} for packaging", path.display()))?;
    file.write_all(chunk_bytes)
        .with_context(|| format!("failed to append bytecode to {}", path.display()))?;
    file.write_all(EMBEDDED_APP_MARKER)
        .with_context(|| format!("failed to append package marker to {}", path.display()))?;
    file.write_all(&(chunk_bytes.len() as u64).to_le_bytes())
        .with_context(|| format!("failed to append package length to {}", path.display()))?;
    Ok(())
}

fn embedded_chunk_from_current_exe() -> Result<Option<Chunk>> {
    let current_exe =
        std::env::current_exe().context("failed to locate current Ricochet executable")?;
    let bytes = fs::read(&current_exe)
        .with_context(|| format!("failed to read {}", current_exe.display()))?;
    embedded_chunk_from_bytes(&bytes)
        .with_context(|| format!("failed to load embedded app from {}", current_exe.display()))
}

fn embedded_chunk_from_bytes(bytes: &[u8]) -> Result<Option<Chunk>> {
    let trailer_len = EMBEDDED_APP_MARKER.len() + 8;
    if bytes.len() < trailer_len {
        return Ok(None);
    }

    let length_start = bytes.len() - 8;
    let marker_start = length_start - EMBEDDED_APP_MARKER.len();
    if &bytes[marker_start..length_start] != EMBEDDED_APP_MARKER {
        return Ok(None);
    }

    let mut length_bytes = [0_u8; 8];
    length_bytes.copy_from_slice(&bytes[length_start..]);
    let chunk_len = u64::from_le_bytes(length_bytes) as usize;
    if marker_start < chunk_len {
        bail!("embedded Ricochet app length exceeds executable size");
    }
    let chunk_start = marker_start - chunk_len;
    let chunk = Chunk::from_bytes(&bytes[chunk_start..marker_start])?;
    Ok(Some(chunk))
}

fn format_path(path: &str, check: bool) -> Result<()> {
    let path = Path::new(path);
    if path.is_file() {
        let changed = format_file(path, check)?;
        if check && changed {
            bail!("format check failed");
        }
        return Ok(());
    }
    if !path.is_dir() {
        bail!("fmt path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    collect_rco_files(path, &mut files)?;
    files.sort();

    let mut changed = false;
    for file in files {
        changed |= format_file(&file, check)?;
    }
    if check && changed {
        bail!("format check failed");
    }

    Ok(())
}

fn format_file(path: &Path, check: bool) -> Result<bool> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let formatted =
        format_source(&source).with_context(|| format!("failed to format {}", path.display()))?;
    if source == formatted {
        if check {
            println!("checked {}", path.display());
        }
        return Ok(false);
    }

    if check {
        eprintln!("{} would reformat", path.display());
        return Ok(true);
    }

    fs::write(path, formatted).with_context(|| format!("failed to write {}", path.display()))?;
    println!("formatted {}", path.display());
    Ok(true)
}

fn compile_source_file(source_path: &Path) -> Result<Chunk> {
    compile_file_with_imports(source_path)
}

fn collect_test_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        bail!("test path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    collect_rco_files(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rco_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", path.display()))?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_rco_files(&entry_path, files)?;
        } else if entry_path.extension().and_then(|ext| ext.to_str()) == Some("rco") {
            files.push(entry_path);
        }
    }
    Ok(())
}
