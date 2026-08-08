use std::{env, fs, process::ExitCode};

use ricochet2_surface_prototype::{analyze, format_source, line_and_column, Analysis};

const VALID_PROOF: &str = include_str!("../fixtures/typed_postfix.ricochet");
const INVALID_PROOF: &str = include_str!("../fixtures/invalid_surface.ricochet");

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        None | Some("demo") => demo(),
        Some("check") => {
            let path = required_path(arguments.next())?;
            let source = read_source(&path)?;
            let analysis = analyze(&source);
            print_diagnostics(&path, &source, &analysis);
            if analysis.has_errors() {
                Err(format!("check failed with {} diagnostic(s)", analysis.diagnostics.len()))
            } else {
                println!(
                    "PASS {path}: {} tokens, {} declarations",
                    analysis.cst.tokens.len(),
                    analysis.ast.declaration_count()
                );
                Ok(())
            }
        }
        Some("inspect") => {
            let path = required_path(arguments.next())?;
            let source = read_source(&path)?;
            let analysis = analyze(&source);
            print_analysis(&path, &source, &analysis);
            if analysis.has_errors() {
                Err("inspect found syntax errors".to_string())
            } else {
                Ok(())
            }
        }
        Some("format") => {
            let path = required_path(arguments.next())?;
            let source = read_source(&path)?;
            print!("{}", format_source(&source));
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown command '{command}'\nusage: ricochet2-surface-proof [demo|check PATH|inspect PATH|format PATH]"
        )),
    }
}

fn demo() -> Result<(), String> {
    println!("Ricochet 2 / ADR-001 typed-postfix surface proof");
    println!("=================================================");

    let valid = analyze(VALID_PROOF);
    let recovered = valid.cst.recover_source() == VALID_PROOF;
    let formatted = format_source(VALID_PROOF);
    let idempotent = format_source(&formatted) == formatted;
    println!("\nVALID CORPUS");
    println!("  lossless CST recovery : {}", pass_fail(recovered));
    println!("  formatter idempotence: {}", pass_fail(idempotent));
    println!("  diagnostics           : {}", valid.diagnostics.len());
    println!("  tokens                : {}", valid.cst.tokens.len());
    println!(
        "  declarations          : {}",
        valid.ast.declaration_count()
    );
    println!("  typed stack rows:");
    for declaration in valid.ast.declarations() {
        if let Some(signature) = &declaration.signature {
            println!("    {:<22} {}", declaration.name, signature.stack_row());
        }
    }

    let invalid = analyze(INVALID_PROOF);
    println!("\nDELIBERATELY INVALID CORPUS");
    print_diagnostics("fixtures/invalid_surface.ricochet", INVALID_PROOF, &invalid);

    let expected_codes = ["R2P016", "R2P030", "R2L004", "R2L005", "R2P007"];
    let actual_codes = invalid
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    let passed =
        recovered && idempotent && valid.diagnostics.is_empty() && actual_codes == expected_codes;
    println!("\nOVERALL: {}", pass_fail(passed));
    println!("This is preserved architecture evidence, not the production Ricochet 2 parser.");
    if passed {
        Ok(())
    } else {
        Err("ADR-001 proof failed".to_string())
    }
}

fn print_analysis(path: &str, source: &str, analysis: &Analysis) {
    println!("source: {path}");
    println!("lossless: {}", analysis.cst.recover_source() == source);
    println!("tokens: {}", analysis.cst.tokens.len());
    println!("declarations: {}", analysis.ast.declaration_count());
    for declaration in analysis.ast.declarations() {
        println!(
            "- {} {} modifiers={:?} generics={:?}",
            declaration.kind, declaration.name, declaration.modifiers, declaration.generics
        );
        if let Some(signature) = &declaration.signature {
            println!("  stack: {}", signature.stack_row());
        }
    }
    print_diagnostics(path, source, analysis);
}

fn print_diagnostics(path: &str, source: &str, analysis: &Analysis) {
    for diagnostic in &analysis.diagnostics {
        let (line, column) = line_and_column(source, diagnostic.span.start);
        println!(
            "{path}:{line}:{column}: {}[{}]: {} (bytes {}..{})",
            diagnostic.severity,
            diagnostic.code,
            diagnostic.message,
            diagnostic.span.start,
            diagnostic.span.end
        );
    }
}

fn required_path(path: Option<String>) -> Result<String, String> {
    path.ok_or_else(|| "this command requires a source path".to_string())
}

fn read_source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("could not read {path}: {error}"))
}

fn pass_fail(value: bool) -> &'static str {
    if value {
        "PASS"
    } else {
        "FAIL"
    }
}
