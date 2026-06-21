pub mod compiler;
pub mod imports;

pub use compiler::{
    compile_source, compile_source_with_imported_macros,
    compile_source_with_imported_macros_and_module_id, expand_module,
    expand_module_with_imported_macros, expand_source, expand_source_with_imported_macros,
    exported_macro_table_from_source, exported_macro_table_from_source_with_imports,
    format_compile_error, CompileError, ImportedMacroTable, MacroExpansion,
    MacroExpansionTraceEntry, MacroImportSummary, MacroPackageMetadata, MacroSourceKind,
    MacroSummary, MacroTableSummary,
};
pub use imports::{
    compile_file_with_imports, expand_file_with_imports, resolve_import_with_metadata,
    verify_runtime_import_locks_for_parent, FileMacroExpansion, ResolvedImport, ResolvedImportKind,
};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
