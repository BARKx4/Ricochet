# Compile-Time Macro Design

Date: 2026-06-20
Status: Design spec for future implementation

## Purpose

Ricochet macros are a compile-time source transformation feature. They let
Ricochet code generate or transform parsed Ricochet syntax while preserving the
language's postfix shape, deterministic builds, and source-aware diagnostics.

This spec is intentionally a design contract only. It does not make macros a
public reference feature until parser, compiler, CLI, LSP, package, and
diagnostic tests land.

## Goals

- Keep declaration and invocation syntax postfix/RPN-shaped.
- Keep ordinary runtime word dispatch unambiguous.
- Expand before bytecode generation, with useful source spans.
- Make hygiene predictable for locals, generated symbols, imported macros, and
  declarations such as `function`, `Subclass`, `Method`, `Accessor`, and
  `Table`.
- Keep initial beta macros pure and deterministic: no host capabilities, no
  runtime side effects, no arbitrary IO.
- Provide expansion traces that can feed `rco expand`, compiler diagnostics,
  and LSP diagnostics.

## Non-Goals For The Initial Beta

- No trusted build hooks.
- No runtime imports or runtime macro loading.
- No arbitrary host IO, environment access, network access, process spawning,
  database access, wall-clock time, randomness, or filesystem reads from macro
  bodies.
- No procedural Rust plugins.
- No direct bytecode or compiler-IR macros.
- No leading-dot macro syntax, fake namespace-dot host APIs, or receiver-first
  pseudo-object calls.

## Syntax

### Declaration

Macro declarations use the same name-first declaration pattern as classes,
methods, functions, variables, and imports:

```ricochet
"unless" Macro
  ( condition body -> expansion )
  [
    [ $condition ast_splice not if $body ast_splice call end ] quote_ast
  ]
end
```

The declaration operator is `Macro`. The macro name is a string literal in the
initial beta. A future implementation may accept a bare static symbol in the
same way `Subclass` does, but the beta should start with string names so
multiword names stay clearly underscore-separated, for example
`"define_route" Macro`.

The `Args` object is optional. A macro without `Args` takes no operands. The
body block is required and `end` is required. `Args`, when present, describes
the invocation stack shape. Unlike runtime functions, macro arguments are
binding metadata for the macro expander: each input name binds to the
corresponding AST operand captured from the invocation. The rightmost input
name receives the top invocation operand, following normal Ricochet
stack-effect convention.

Doc comments attach to the macro declaration exactly as they attach to
`function`, `Subclass`, and `Method` declarations. A doc comment immediately
before `"name" Macro` becomes macro documentation and is exported with the
macro table.

Formal beta grammar:

```text
MacroDecl       ::= Docs? StringLiteral "Macro" Newline* MacroArgs? MacroBody "end"
MacroArgs       ::= ArgsLiteral Newline*
MacroBody       ::= BlockLiteral Newline*
MacroInvocation ::= MacroOperand* MacroName "macro_call"
MacroOperand    ::= ParsedExpr | ParsedBlock | ParsedItem
MacroName       ::= StringLiteral | MacroHandle
```

`StringLiteral` is required for `MacroDecl` names in the beta. Bare symbol names
are deliberately deferred until a future design proves they do not create
confusion with ordinary runtime word dispatch or existing declaration parsing.
`MacroHandle` is compiler metadata from a macro table, not a runtime value.

The macro body is a compile-time block evaluated by the restricted macro
evaluator. It consumes bound AST operands and returns one expansion value:

- an expression sequence for expression positions;
- one or more items for top-level and class-body positions;
- a compile-time error value.

`quote_ast` in examples is a future compile-time helper word, not an existing
runtime word. Compile-time macro helper words should use ordinary public word
shape, with underscores for multiword names such as `quote_ast`,
`macro_error`, `macro_gensym`, and `macro_ident`.

### Invocation

Macro invocation is explicit and word-like:

```ricochet
$ready [
  "not ready" println
] "unless" macro_call
```

`macro_call` is the only syntax that invokes a macro in the initial beta. It
consumes the macro name plus the number of AST operands declared by the macro's
`Args` input list. The macro name may be a string literal or an imported macro
handle produced by the compiler, but not an ordinary runtime value.

This model avoids ambiguity with ordinary runtime word dispatch:

```ricochet
unless          (( ordinary runtime word lookup, if defined ))
"unless" macro_call  (( compile-time macro expansion ))
```

The compiler expands `macro_call` only during compilation. If a macro name is
not known at compile time, compilation fails with a macro-resolution diagnostic.

Package-qualified macro names stay string-shaped and path-like, not dotted.
They use the import specifier plus `#` plus the macro name:

```ricochet
"auth/macros" import
$request [ "admin" require_role ] "auth/macros#route_guard" macro_call
```

The beta does not add implicit macro imports or runtime lookup.

## Formal Contracts

### Macro Declaration Contract

The parser must recognize `Macro` as a declaration operator only in the concrete
forms described above:

```ricochet
(( docs attach here ))
"name" Macro
  [
    ...compile-time macro body...
  ]
end

"name" Macro
  ( input_a input_b -> expansion )
  [
    ...compile-time macro body...
  ]
end
```

No other beta form is accepted. In particular, these are errors:

```ricochet
name Macro               (( bare symbol names are deferred ))
"name" Macro end         (( body block is required ))
"name" Macro [ ... ]     (( end is required ))
"name" .Macro            (( leading-dot form is forbidden ))
```

### Macro Table Construction

Each parsed module produces a macro table before ordinary bytecode lowering.
The stable serialized shape for tools and package metadata is:

```json
{
  "schema_version": 1,
  "module_id": "app/macros",
  "source_hash": "sha256:...",
  "macros": [
    {
      "name": "route_guard",
      "qualified_name": "app/macros#route_guard",
      "canonical_name": "app/macros#route_guard",
      "exported": true,
      "docs": ["Require a role before running a route block."],
      "args": {
        "inputs": ["request", "body"],
        "outputs": ["expansion"]
      },
      "definition_span": {
        "file": "app/macros.rco",
        "start": 0,
        "end": 128,
        "line": 1,
        "column": 1
      },
      "definition_hash": "sha256:..."
    }
  ]
}
```

Top-level macro names that do not start with `_` are exported. Top-level names
starting with `_` are module-local. A module must not define the same macro name
twice. Duplicate local macro declarations are compile errors even when the
definitions are textually identical.

Logical `module_id` values must be normalized:

- local project files use project-root-relative paths without `.rco`, for
  example `app/macros`;
- package files use `<package-name>@<locked-version-or-source-hash>/<module>`,
  for example `auth@sha256:abc123/macros`;
- separators are `/` on every platform;
- stable IDs must not contain absolute machine-local paths, drive letters,
  temp directories, user names, or OS-specific separators.

### Import And Lookup Contract

Macro imports use the existing static import syntax:

```ricochet
"app/macros" import
"auth/macros" import
```

The compiler parses imported modules, builds their macro tables, and makes only
exported macros visible to the importing module's macro resolver. Runtime
imports do not participate.

The user-facing qualified macro specifier is:

```text
<import-specifier>#<macro-name>
```

`<import-specifier>` is the exact string used in a static import, such as
`"auth/macros" import`. `#` is reserved as the qualified macro delimiter and is
not allowed inside macro names or import specifiers used for macro
qualification. The compiler resolves the import specifier to a canonical
normalized `module_id` internally, but users do not type canonical module IDs.
This keeps package authors away from lockfile/source-hash details such as
`auth@sha256:abc123/macros`.

Lookup rules for `"name" macro_call`:

1. If the current module defines `name`, use that local macro.
2. Otherwise, search exported macros from statically imported modules.
3. If exactly one imported macro has that unqualified name, use it.
4. If zero imported macros match, emit an unknown-macro diagnostic.
5. If more than one imported macro matches, emit an ambiguous-macro diagnostic.

Lookup rules for `"<import-specifier>#name" macro_call`:

1. Split on the final `#`.
2. Match the left side against static import specifiers visible in the current
   module, or the reserved local specifier `self`.
3. Resolve that import specifier to exactly one canonical normalized `module_id`.
4. Require exactly one matching exported macro named `name`; `self#name` may
   also name a local macro.
5. Emit an unknown-qualified-macro diagnostic if the import specifier, module,
   or macro name is missing.

Canonical `module_id` values and canonical names are for trace/cache data, not
for source syntax. A canonical package module ID may contain `:` as part of a
hash label, so it is never parsed by splitting source text on `:`.

Package macro resolution must follow the same containment rules as ordinary
package imports. The beta implementation must read `ricochet.toml` to locate
dependencies and must read `ricochet.lock` when it is present so trace and cache
data can record the locked package identity. Until implementation explicitly
verifies lockfile integrity for macro expansion, the spec must not claim that
macros are cryptographically pinned or that package revision drift is fully
prevented. The initial guarantee is traceability and deterministic IDs from the
package source actually used.

### Evaluator Word Resolution Contract

Macro bodies run in a separate compile-time evaluator namespace. The namespace
contains only:

- macro arguments and macro-local bindings;
- literal values and quoted AST values;
- an allowlisted compile-time word table;
- imported macro definitions as metadata, not callable runtime helpers.

The allowlisted table is explicit and fail-closed. Initial beta words should be
limited to deterministic AST and diagnostic helpers:

```text
quote_ast
ast_symbol
ast_string
ast_number
ast_reference
ast_block
ast_sequence
ast_item
ast_splice
macro_error
macro_gensym
macro_ident
macro_span
macro_arg
macro_eq?
macro_list
macro_map
macro_each
macro_format
```

Any word not in this allowlist is a compile-time error inside macro bodies. That
includes ordinary runtime words, user-defined functions, imported package
runtime helpers, and host capability words.

### Stable JSON Contract

Every macro-facing JSON object uses `schema_version`. Stable fields are:

- normalized `module_id`;
- `source_hash`;
- macro table `name`, user-facing `qualified_name`, `canonical_name`,
  `exported`, `args`, `definition_span`, `definition_hash`, and `docs`;
- expansion trace IDs;
- AST/tree JSON node kinds and child order;
- diagnostics codes, spans, messages, and expansion stacks;
- input, import, macro definition, and output hashes.

`expanded_source` is parseable output, not a whitespace golden. It is stable
only under a declared formatter version and `schema_version`.

Expansion trace IDs are stable logical IDs derived from the normalized root
module ID, invocation byte span, expansion depth, and macro canonical name. They
must not include absolute paths or process-local counters that change across
equivalent builds.

### Quote And Splice Semantics

`quote_ast` converts a quoted block literal into AST without executing the
quoted Ricochet code. Names inside the quoted block are ordinary generated
Ricochet syntax by default:

```ricochet
[
  [ $name println ] quote_ast
]
```

The generated AST above contains an ordinary `$name` static read. It does not
splice a macro argument named `name`.

Splicing is explicit. A macro argument or macro-local AST value is inserted into
quoted output only when it is followed by `ast_splice` inside the quoted block:

```ricochet
[
  [ $name ast_splice println ] quote_ast
]
```

`ast_splice` consumes a compile-time AST value and replaces itself with that
value in the quoted output. If the binding is not an AST value of the kind valid
at that position, expansion fails with a splice-kind diagnostic that points to
the splice site and the macro invocation.

Splicing never changes the caller binding rules of the spliced syntax. If the
caller supplied `$ready`, the expanded AST still reads the caller's `ready`
binding; it is not rebound to a macro-local variable named `ready`.

## Worked Examples

### Expression Macro

Definition:

```ricochet
(( Run a block when a condition is false. ))
"unless" Macro
  ( condition body -> expansion )
  [
    [ $condition ast_splice not if $body ast_splice call end ] quote_ast
  ]
end
```

Invocation:

```ricochet
$ready [
  "not ready" println
] "unless" macro_call
```

Expansion shape:

```ricochet
$ready not if
  [
    "not ready" println
  ] call
end
```

### Class-Body Macro

Definition:

```ricochet
(( Add an Accessor for one field. ))
"model_accessor" Macro
  ( field_name -> expansion )
  [
    [
      $field_name ast_splice Accessor
    ] quote_ast
  ]
end
```

Invocation inside a class body:

```ricochet
User Model Subclass
  "users" Table
  "email" "model_accessor" macro_call
end
```

Expansion shape:

```ricochet
User Model Subclass
  "users" Table
  "email" Accessor
end
```

## Expansion Target

Macros expand parsed AST, not raw tokens and not compiler IR.

The expansion pass runs after lexing/parsing and import discovery, before
declaration lowering and bytecode generation. The input and output values are
based on the existing syntax model: module items, class-body items, expression
sequences, blocks, args, symbols, references, literals, and spans.

AST expansion is the beta target because:

- token-stream macros make hygiene and balanced block handling harder;
- compiler-IR macros run too late to generate declarations like `Subclass`,
  `Method`, `Accessor`, `Table`, and `function`;
- AST nodes already carry source spans, which lets diagnostics point to both
  invocation source and expansion source;
- AST output can be formatted, linted, and presented by `rco expand` and LSP;
- deterministic expansion can be cached from parsed source, normalized module
  IDs, imports, traceable lockfile data when present, compiler version, and
  macro definitions.

The macro API must not expose bytecode operations in the initial beta. If IR
macros are ever designed later, they should be a separate trusted extension.

## Expansion Phases

Compilation with macros has these phases:

1. Lex and parse source into AST, including `Macro` declarations and explicit
   `macro_call` expressions.
2. Resolve static imports and package imports through the existing manifest
   rules, reading lockfile data when present for trace/cache identity.
3. Build a compile-time macro table from the current module and statically
   imported modules.
4. Expand macros to a new AST, recording an expansion trace and source map.
5. Re-run declaration validation and normal compiler lowering on the expanded
   AST.
6. Emit bytecode with source spans that can point through generated nodes back
   to invocation and macro definition spans.

Macro declarations do not emit runtime bytecode. A file that only declares
macros should compile to an empty runtime chunk plus macro metadata for import.

## Hygiene

### Local Names

Macro body locals are compile-time locals. They are not runtime locals and they
do not appear in expanded user code unless the macro explicitly quotes or
constructs identifiers.

Inside a macro body, `$name` reads a compile-time macro binding when `name` is
a macro argument or compile-time local. In quoted output AST, `$name` remains an
ordinary Ricochet static read unless it is explicitly spliced from a macro
binding.

Caller syntax passed as an operand keeps the caller's lexical scope and spans.
A macro that splices `condition` into output should preserve the original
operand's binding behavior rather than rebinding it at the macro definition.

### Generated Symbols

Generated names are hygienic by default. A macro must use `macro_gensym` when
it needs a temporary variable, function, class, or helper method that should not
capture or be captured by caller code.

`macro_gensym` is deterministic. Its stable identity is derived from:

- macro definition identity;
- normalized logical module ID;
- invocation source span;
- expansion index within the invocation;
- optional caller-provided hint.

The normalized logical module ID uses the same rules as the macro table
contract: project-root-relative `/` paths for local files, package
name/version-or-source-hash plus module path for package files, and no absolute
machine-local path components.

The printed fallback form may look like `__macro_<hash>_<hint>`, but source
tests must compare stable trace IDs rather than relying on the exact fallback
spelling.

### Intentional Public Names

When a macro is meant to declare a public `function`, class, `Method`,
`Accessor`, or `Table`, it must emit the exact public name as source syntax from
a literal or caller operand. Generated symbols are private unless the macro
explicitly converts them into public identifiers.

Name conflicts are diagnosed after expansion by the same declaration checks that
ordinary source uses. Method replacement keeps the existing replacement
metadata behavior.

### Imports And Packages

Macro definitions are module-scoped and are exported, imported, and resolved by
the formal macro table and lookup contracts above. Static imports make exported
macro definitions available to the importer during compilation only. Qualified
lookup uses `"<import-specifier>#name" macro_call`, such as
`"auth/macros#route_guard" macro_call`.

Package macros are resolved through the same manifest and containment model as
ordinary package imports. Lockfile data must be included in traces and cache
keys when present, but the beta design does not claim package-stability
guarantees beyond the verification the implementation actually performs.

Dynamic runtime imports do not load macros in the beta.

### Existing Declarations

Macros expand before ordinary declaration lowering. Therefore a macro may emit
the same AST shapes a user could write by hand:

```ricochet
User Model Subclass
  "users" Table
  "email" Accessor

  [ self email.get ] "displayName" Method
end
```

A macro invoked in a class body may emit `Table`, `Accessor`, `Field`, or
`Method` items that target the surrounding class. A macro invoked outside a
class body must emit the explicit target form whenever the ordinary declaration
requires it.

A macro may emit `function` declarations at top level, but not inside a class
body unless ordinary Ricochet source supports that placement at the time the
macro feature lands.

## Compile-Time Safety

The beta macro evaluator has no host capabilities and does not share the
runtime global word namespace. Its value model is separate from runtime values:

- scalar literals: strings, numbers, booleans, nil;
- immutable compile-time lists and maps;
- macro handles;
- AST values;
- source-span values;
- diagnostic values.

Runtime objects, class instances, resources, sockets, database handles,
processes, tasks, closures, and host capability handles are not macro values.

Macro bodies can use only the allowlisted compile-time words from the formal
contract:

- AST construction and inspection helpers;
- deterministic string, number, boolean, collection, and symbol operations;
- deterministic compile-time errors through `macro_error`;
- deterministic `macro_gensym`;
- pure formatting helpers for diagnostics.

Resolution is fail-closed. If a word inside a macro body is not in the
allowlisted compile-time word table, compilation fails even when a runtime word,
user-defined function, imported package helper, or native host capability with
the same name exists.

Macro bodies cannot:

- call runtime functions or user-defined functions;
- call imported package runtime helpers;
- execute user bytecode with side effects;
- access host capabilities;
- read or write files through `fs_*` words;
- make HTTP or socket calls through `http_*` or networking words;
- inspect environment variables;
- run processes, shells, PTYs, or tasks;
- access databases or migrations;
- observe time or randomness.

If a future trusted build-hook design is added, it must be opt-in, separately
named, and excluded from the initial `Macro` evaluator.

## Limits And Determinism

The compiler must enforce hard expansion limits:

- maximum macro expansion depth: 32 nested invocations;
- maximum recursive invocations of the same macro in one expansion chain: 8;
- maximum generated AST nodes per source file: implementation default 100,000;
- maximum expanded source bytes for `rco expand`: implementation default 4 MiB;
- maximum macro evaluator steps per invocation: implementation default 10,000.

The exact defaults may become CLI-configurable later, but tests should pin the
initial defaults.

Expansion is deterministic. The cache key should include:

- compiler version and macro trace schema version;
- normalized logical module ID and source hash for the root source;
- normalized logical module IDs and source hashes for imported modules;
- `ricochet.toml` dependency declarations relevant to imports;
- `ricochet.lock` package identity data when present;
- macro definition AST and source hash;
- expansion flags that affect output.

The cache must never include wall-clock time, absolute temp paths, process IDs,
random values, host environment, network state, drive letters, user home
directories, OS-specific separators, or absolute machine-local source paths.

## Diagnostics And Source Maps

Macro diagnostics must show both where a macro was invoked and where the
expansion was produced. A compiler error inside generated output should include
an expansion stack:

```text
error[E-macro-expansion]: unknown word `email.get`
  at app/controllers/home.rco:12:5
  expanded from "define_route" macro_call at app/controllers/home.rco:8:3
  macro defined at packages/web/macros.rco:3:1
  generated by packages/web/macros.rco:7:9
```

The expansion source-map format should be JSON-serializable and stable enough
for CLI and LSP consumers:

```json
{
  "schema_version": 1,
  "expansions": [
    {
      "id": "expansion-1",
      "macro": "define_route",
      "invocation": {
        "file": "app/controllers/home.rco",
        "start": 128,
        "end": 168,
        "line": 8,
        "column": 3
      },
      "definition": {
        "file": "packages/web/macros.rco",
        "start": 32,
        "end": 220,
        "line": 3,
        "column": 1
      },
      "generated": [
        {
          "node_id": "node-17",
          "generated_span": {
            "file": "<macro:define_route>",
            "start": 0,
            "end": 42,
            "line": 1,
            "column": 1
          },
          "origin": {
            "file": "packages/web/macros.rco",
            "start": 96,
            "end": 140,
            "line": 7,
            "column": 9
          }
        }
      ]
    }
  ]
}
```

Position conventions:

- `start` and `end` are UTF-8 byte offsets from the start of the file.
- `end` is exclusive.
- `line` and `column` are 1-based CLI/display positions derived from the same
  UTF-8 byte offset convention as current source diagnostics.
- LSP adapters convert these spans to zero-based UTF-16 LSP positions at the
  protocol boundary. LSP JSON must not leak byte columns as `character` values.

The compiler can adapt this into the current `SourceSpan` model for bytecode by
recording a primary generated span plus an optional expansion stack for tools
that know how to display it. Tools that only understand `SourceSpan` should
still point at the macro invocation.

## `rco expand`

`rco expand` prints macro-expanded Ricochet without executing runtime code.

Initial stable test surface:

```bash
rco expand path/to/file.rco --json
```

The JSON output is the stable contract for tests, but not every field has the
same stability promise.

Stable fields:

- `schema_version`;
- normalized input `module_id`;
- input `source_hash`;
- compiler and formatter versions;
- normalized import and macro table summaries;
- normalized expanded AST/tree JSON;
- expansion trace IDs and source-map entries;
- diagnostics, including codes, messages, spans, and expansion stacks;
- cache and output hashes.

Parseable but not whitespace-stable fields:

- `expanded_source`.

`expanded_source` must parse as Ricochet and must be generated by the declared
formatter version, but tests should not golden-file exact whitespace unless they
explicitly pin that formatter version and schema version.

The normalized AST/tree JSON is the preferred stable shape for tests. This
sample is representative rather than exhaustive:

```json
{
  "schema_version": 1,
  "module_id": "app/controllers/home",
  "source_hash": "sha256:...",
  "compiler_version": "ricochet/0.1.x",
  "formatter_version": "ricochet-fmt/1",
  "imports": [
    {
      "specifier": "auth/macros",
      "module_id": "auth@sha256:abc123/macros",
      "source_hash": "sha256:..."
    }
  ],
  "macro_tables": [
    {
      "module_id": "auth@sha256:abc123/macros",
      "macros": [
        {
          "name": "route_guard",
          "qualified_name": "auth/macros#route_guard",
          "canonical_name": "auth@sha256:abc123/macros#route_guard"
        }
      ]
    }
  ],
  "expanded_ast": {
    "kind": "module",
    "items": [
      {
        "kind": "class",
        "name": "User",
        "superclass": "Model"
      }
    ]
  },
  "expanded_source": "User Model Subclass\nend\n",
  "trace": [
    {
      "id": "expansion:app/controllers/home:128:0:auth@sha256:abc123/macros#route_guard",
      "macro": "auth@sha256:abc123/macros#route_guard"
    }
  ],
  "diagnostics": [],
  "cache_hash": "sha256:...",
  "output_hash": "sha256:..."
}
```

Human-readable output may be nicer but is not the primary test contract:

```bash
rco expand path/to/file.rco
rco expand path/to/file.rco --trace
rco expand path/to/file.rco --format source
rco expand path/to/file.rco --format tree
```

Later flags may include:

- `--no-cache`;
- `--show-spans`;
- `--show-hygiene`;
- `--max-depth <n>`;
- `--max-nodes <n>`;
- `--package-lock <path>`;
- `--lsp-json`.

Pretty expanded source should preserve postfix formatting and avoid inventing
non-canonical syntax. Formatter changes can alter human output, so tests should
prefer JSON trace and parseable expanded source over exact whitespace.

## LSP And Editor Behavior

The LSP must parse `Macro` declarations even before full expansion is available.
At minimum it should:

- identify macro declaration ranges;
- format macro declaration blocks with normal Ricochet indentation;
- validate postfix declaration shape;
- diagnose leading-dot and fake namespace-dot forms inside macro bodies using
  the same syntax guardrails as ordinary source;
- diagnose unknown or ambiguous macro names when the macro table can be built
  cheaply;
- surface "macro expansion unavailable" as a diagnostic when the editor cannot
  safely run full expansion.

If full expansion is unavailable in LSP, the editor must not silently assume the
expanded program is valid. It should still provide best-effort parse diagnostics
for the original source and macro definitions.

LSP formatting should not expand macros. Expansion is a compile-time analysis
operation, not a source rewrite.

## Testing Strategy For Implementation

Future implementation should add focused tests for:

- parser support for `Macro` declarations and `macro_call` invocations;
- rejection of non-postfix or leading-dot macro syntax;
- compiler expansion into expression sequences, top-level items, and class-body
  items;
- deterministic expansion across repeated builds;
- recursion, depth, node, byte, and evaluator-step limits;
- generated-symbol hygiene and caller-scope operand splicing;
- interaction with `$name` static reads in macro bodies and generated output;
- expansion of `function`, `Subclass`, `Method`, `Accessor`, and `Table`;
- static import and package import behavior for macros;
- ambiguous imported macro names;
- normalized logical module IDs for local and package macros;
- import-specifier `#` qualified lookup without canonical-ID delimiter
  conflicts;
- lockfile data included in traces/cache keys when present;
- diagnostics that include invocation, definition, and generated-source origins;
- source-map UTF-8 byte offsets, 1-based CLI positions, and LSP UTF-16
  conversion;
- `rco expand --json` schema stability for normalized AST/tree, trace IDs,
  diagnostics, and hashes;
- `expanded_source` parseability without golden-whitespace dependence;
- LSP parse, formatting, diagnostics, and unsupported-expansion diagnostics;
- denial of host capabilities from macro bodies, including `fs_*`, `http_*`,
  process, env, time, randomness, database, and user runtime functions;
- proof that runtime imports cannot load or execute macros.

Full verification for the implementation epic should include the ordinary
workspace checks plus docs validation after public docs are updated. This spec
alone does not require cargo tests.
