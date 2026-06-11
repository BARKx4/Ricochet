use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use ricochet_bytecode::{Chunk, Op};

use crate::compile_source;

pub fn compile_file_with_imports(source_path: impl AsRef<Path>) -> Result<Chunk> {
    SourceResolver::default().compile_file(source_path.as_ref())
}

#[derive(Default)]
struct SourceResolver {
    loaded: BTreeSet<PathBuf>,
    visiting: BTreeSet<PathBuf>,
}

impl SourceResolver {
    fn compile_file(&mut self, source_path: &Path) -> Result<Chunk> {
        let canonical = fs::canonicalize(source_path)
            .with_context(|| format!("failed to resolve {}", source_path.display()))?;
        if self.loaded.contains(&canonical) {
            return Ok(Chunk::new(source_path.to_string_lossy()));
        }
        if !self.visiting.insert(canonical.clone()) {
            bail!("cyclic Ricochet import involving {}", source_path.display());
        }

        let (file, source) = read_source_path(source_path)?;
        let imports = static_imports(&source)
            .with_context(|| format!("failed to scan imports in {}", source_path.display()))?;
        let source_without_imports = strip_static_imports(&source)?;
        let mut combined = Chunk::new(file.clone());
        let parent = source_path.parent().unwrap_or_else(|| Path::new("."));

        for import in imports {
            let import_path = resolve_import(parent, &import);
            let chunk = self.compile_file(&import_path).with_context(|| {
                format!("failed to import {import:?} from {}", source_path.display())
            })?;
            append_chunk(&mut combined, chunk);
        }

        let own_chunk = compile_source(&file, &source_without_imports)?;
        append_chunk(&mut combined, own_chunk);
        self.visiting.remove(&canonical);
        self.loaded.insert(canonical);
        Ok(combined)
    }
}

fn read_source_path(source_path: &Path) -> Result<(String, String)> {
    let source = fs::read_to_string(source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let file = source_path.to_string_lossy();

    Ok((file.into_owned(), source))
}

fn static_imports(source: &str) -> Result<Vec<String>> {
    source
        .lines()
        .filter_map(|line| parse_static_import_line(line.trim()))
        .collect()
}

fn strip_static_imports(source: &str) -> Result<String> {
    let mut stripped = String::new();
    for line in source.lines() {
        if parse_static_import_line(line.trim()).transpose()?.is_some() {
            stripped.push('\n');
        } else {
            stripped.push_str(line);
            stripped.push('\n');
        }
    }
    Ok(stripped)
}

fn parse_static_import_line(line: &str) -> Option<Result<String>> {
    let rest = line.strip_prefix('"')?;
    let (value, rest) = match parse_string_prefix(rest) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => return Some(Err(anyhow::anyhow!("unterminated import string"))),
        Err(error) => return Some(Err(error)),
    };
    if rest.trim() == "import" {
        Some(Ok(value))
    } else {
        None
    }
}

fn parse_string_prefix(source: &str) -> Result<Option<(String, &str)>> {
    let mut value = String::new();
    let mut escape = false;
    for (index, ch) in source.char_indices() {
        if escape {
            let decoded = match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => other,
            };
            value.push(decoded);
            escape = false;
            continue;
        }

        match ch {
            '\\' => escape = true,
            '"' => return Ok(Some((value, &source[index + 1..]))),
            ch => value.push(ch),
        }
    }

    if escape {
        bail!("unterminated import string escape");
    }
    Ok(None)
}

fn resolve_import(parent: &Path, import: &str) -> PathBuf {
    let import_path = Path::new(import);
    let mut path = if import_path.is_absolute() {
        import_path.to_path_buf()
    } else {
        parent.join(import_path)
    };
    if path.extension().is_none() {
        path.set_extension("rco");
    }
    path
}

fn append_chunk(target: &mut Chunk, chunk: Chunk) {
    let instruction_offset = target.instructions.len();
    let block_offset = target.blocks.len();

    target.blocks.extend(chunk.blocks);
    target
        .instructions
        .extend(chunk.instructions.into_iter().map(|mut instruction| {
            instruction.op = rebase_op(instruction.op, instruction_offset, block_offset);
            instruction
        }));
}

fn rebase_op(op: Op, instruction_offset: usize, block_offset: usize) -> Op {
    match op {
        Op::PushBlock(index) => Op::PushBlock(index + block_offset),
        Op::AddMethod { name, block, args } => Op::AddMethod {
            name,
            block: block + block_offset,
            args,
        },
        Op::AddFunction { name, block, args } => Op::AddFunction {
            name,
            block: block + block_offset,
            args,
        },
        Op::JumpIfFalse(target) => Op::JumpIfFalse(target + instruction_offset),
        Op::Jump(target) => Op::Jump(target + instruction_offset),
        op => op,
    }
}
