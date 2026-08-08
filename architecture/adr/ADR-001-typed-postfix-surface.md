# ADR-001: Typed postfix surface

Status: Proposed

Opened: 2026-08-08

Target decision: before Phase 1 parser and type-checker implementation

Depends on: ADR-010

Coordinates with: ADR-002, ADR-004, ADR-005, ADR-006, and ADR-008

## Context

Ricochet 2 needs explicit public types, local inference, generics, traits,
algebraic data, exhaustive matching, effects, async callables, deterministic
resources, visibility, and native boundaries. Most mainstream type syntaxes
place an operator or callee before its operands. Copying one directly would
make declarations look like a second, prefix language attached to a postfix
runtime.

The source surface must also be practical for a lossless parser, formatter,
LSP, debugger, documentation generator, and stable diagnostic spans. Clever
punctuation is not a win if it becomes difficult to read in real applications
or ambiguous when generics, nested blocks, and patterns meet.

This ADR proposes a concrete syntax family for prototypes. It is not accepted
grammar. Examples deliberately cover more than the Phase 1 implementation so
the initial parser is not optimized around a syntax that fails as soon as
traits, async, FFI, or classes arrive.

## Proposed decision

### 1. Executable code remains pure postfix

Runtime operands precede the word that consumes them. Receivers precede
selectors, and selector arguments remain below the receiver:

```ricochet
user email.get
"ada@example.com" user email.set

10 User limit
"email" "ada@example.com" User where

request "method" at
settings "theme" "dark" put

queue job push
```

Declaration punctuation may describe types and patterns, but it does not
change runtime evaluation. Ricochet 2 does not add leading-dot calls,
receiver-first pseudo-object calls, implicit pipeline reversal, or infix
operators inside executable code.

Public multiword words use `_`. `-` remains the subtraction word or a numeric
sign rather than a word separator. Predicate words may retain a final `?` when
the question reads naturally. A final or leading `!` is not a general word
naming convention.

### 2. Declaration heads terminate in the declaration word

A declaration head is itself postfix metadata. The signature, name, and
ordered modifiers precede the terminal declaration word:

```ricochet
( left: Int right: Int -> Int ) add_numbers public function
  $left $right +
end
```

The `function` word receives the signature, name, and modifiers conceptually;
the body ends with lowercase `end`. This keeps the strongest visual cue at the
right edge of the header and preserves the existing Ricochet habit of naming
the operation last.

Canonical modifier order is:

```text
visibility, storage/dispatch, shape, declaration word
```

For example:

```ricochet
( email: Email -> Option<User> uses database ) find_by_email public static Method
  $email User query_first
end

( -> String ) display_name protected final Method
  self name.get
end
```

The parser diagnoses a noncanonical order and the formatter fixes it only when
the rewrite is semantics-preserving. It does not accept many equivalent
spellings indefinitely.

Names in declaration heads are identifier tokens, not runtime string values.
Strings remain ordinary values for dynamic lookup and reflection. This keeps a
misspelled declaration name in the name-resolution system rather than the
runtime string system.

### 3. Callable signatures describe stack inputs, outputs, and effects

The proposed signature form is:

```text
( input_name: InputType ... -> OutputType ... uses effect ... )
```

Inputs are listed from deeper to shallower stack position, matching source
order. In this call, `left` is below `right`:

```ricochet
20 22 add_numbers
```

Inside the callable, named parameters are lexical reads:

```ricochet
$left $right +
```

Output names are optional metadata for documentation; callers still receive
ordinary stack values. Multiple outputs are explicit:

```ricochet
( dividend: Int divisor: Int -> quotient: Int remainder: Int ) div_rem public function
  $dividend $divisor integer_div_rem
end
```

No output is written by placing `uses` or `)` immediately after the arrow:

```ricochet
( message: String -> uses console ) log_line public function
  $message println
end
```

An omitted `uses` clause means an empty effect set. Exported callables list all
nonempty effects. The candidate spelling uses the word `uses` instead of `!`
so the effect boundary is readable and does not revive confusing bang-shaped
public word families:

```ricochet
( url: Url -> Result<Response, HttpError> uses async network ) fetch public function
  $url http_get await
end

( address: RawPtr<U8> -> U8 uses unsafe ) read_foreign_byte private function
  $address pointer_read_u8
end
```

ADR-005 owns the final bounded effect vocabulary and propagation rules. This
ADR owns where that vocabulary appears in source.

Stack-row variables are normally inferred and omitted from source. Expert and
compiler-generated interface views may show the complete row-polymorphic
signature, but ordinary public declarations name only the values they consume
and produce. ADR-002 defines the full internal notation.

### 4. Immutable bindings are the default

Bindings keep value-before-name-before-operation order:

```ricochet
"Ada" name let
42 retry_count var

$name println
$retry_count 1 + retry_count set
```

`let` creates an immutable lexical binding. `var` creates a mutable lexical
cell, and `set` requires one that already exists. The type is inferred unless
the binding is a public/static boundary or an annotation is needed to resolve
an otherwise ambiguous literal:

```ricochet
0 retry_count: U32 var
```

The annotation is declaration metadata attached to the name. It does not
change the runtime stack order. Shadowing is permitted only under an explicit
lint policy; assignment never creates a binding accidentally.

### 5. Generic parameters precede the signature they parameterize

Generic declarations use a visually bounded metadata clause followed by the
ordinary postfix declaration head:

```ricochet
<T: Comparable>
( values: List<T> -> Option<T> ) maximum public function
  $values maximum_by_default_order
end
```

Multiple constraints remain inside the clause rather than becoming executable
words:

```ricochet
<K: Hash + Equal, V>
( source: Map<K, V> key: K fallback: V -> V ) value_or public function
  $source $key at_option
  match
    Some(value) when
      $value
    None when
      $fallback
  end
end
```

The prototype must decide whether associated-type constraints use a dedicated
keyword or scoped type punctuation. It may not use namespace-dot executable
syntax. Higher-kinded type parameters and user-defined type operators are not
part of the 2.0 grammar.

### 6. Records, enums, traits, and classes keep terminal meta words

The candidate nominal data declarations are:

```ricochet
User public Record
  name String public Field
  email Email public Field
  joined_at Instant private Field
end

Option<T> public Enum
  None Case
  Some(T) Case
end

Result<T, E> public Enum
  Ok(T) Case
  Err(E) Case
end
```

Payloads precede their type receiver and variant selector when values are
constructed:

```ricochet
Option None
"Ada" Option Some

$user Result Ok
$error Result Err
```

Record construction likewise keeps values before the nominal receiver and
constructor selector. Exact constructor naming belongs to ADR-004, but the
ordering does not:

```ricochet
"Ada" "ada@example.com" User new
```

Traits and implementations use the same terminal form:

```ricochet
Display public Trait
  ( -> String ) display_name public required Method
end

User Display Implements
  ( -> String ) display_name public Method
    self name.get
  end
end
```

Classes use one superclass and any number of trait implementations:

```ricochet
Account public abstract Class
  id AccountId protected Field
end

User Account public final Subclass
  email Email private Field

  ( -> Email ) email public Method
    self email.get
  end
end
```

`Record`, `Enum`, `Trait`, `Class`, `Subclass`, `Implements`, `Field`, `Case`,
and `Method` are capitalized meta words. Executable words and non-OOP control
words remain lowercase. This preserves the established visual distinction for
OOP declarations without changing lowercase `end`.

Associated/static functions are explicit modifiers. Abstract, sealed/final,
and virtual behavior is declared rather than inferred from whether a body
happens to exist. Arbitrary operator definitions are excluded; standard
operators delegate only to named standard traits selected in ADR-004.

### 7. Pattern matching puts the scrutinee before `match`

Patterns are declaration-like metadata in arm headers. The scrutinee remains a
normal postfix operand:

```ricochet
$result match
  Ok(response) when
    $response body.get
  Err(error) when
    $error log_http_error
    "unavailable"
end
```

Pattern bindings are identifiers in the pattern and `$` reads in the arm.
Closed enums require exhaustive arms. The compiler diagnoses unreachable arms
and duplicate literal/range coverage.

Nested record, tuple, and enum patterns must remain readable when formatted:

```ricochet
$event match
  UserChanged(User(name email)) when
    $name $email notify_user_change
  Shutdown(reason) when
    $reason log_shutdown
end
```

The exact guard spelling is intentionally not frozen in this proposal. The
prototype must compare a guard attached to `when` with a nested ordinary `if`.
It must not evaluate a guard before its pattern bindings exist.

Recoverable-result propagation is a postfix operation:

```ricochet
"config/app.json" fs_read_text try config_text let
$config_text AppConfig json_decode try config let
```

`try` consumes `Result<T, E>`, leaves `T` on success, and returns the compatible
`E` from the current callable on failure. It is not exception syntax and does
not hide an effect. Explicit `match` remains available when either branch needs
work.

### 8. Control flow retains value-before-control order

Ordinary conditions and iterables precede their control word:

```ricochet
$user active.get if
  $user welcome
else
  $user explain_inactive
end

$items each
  item let
  $item process
end
```

Every arm and reachable exit has a statically compatible stack effect. The
surface does not add an implicit expression/value distinction: a block may
leave values only when its checked signature says so.

Blocks remain explicit values where a higher-order word is clearer:

```ricochet
[ 2 * ] $numbers transform
[ 0 > ] $numbers select
```

The collection remains before the selector's block argument according to the
existing selector-argument rule. Formatter and docs examples must use one
canonical order for each public word.

### 9. Async and structured concurrency stay postfix

Async suspension is an effect in the declaration and a postfix operation at
the use site:

```ricochet
( url: Url -> Result<Response, HttpError> uses async network ) fetch public function
  $url http_request await
end
```

Calling an async callable produces a typed task/future value; `await` consumes
that value. Structured work is scoped by applying a scope word to a block:

```ricochet
[
  $primary_url fetch spawn primary_task let
  $backup_url fetch spawn backup_task let

  $primary_task await
  $backup_task await
] task_scope
```

ADR-006 will determine whether `spawn` returns `Task<T>`, how task groups and
select are named, and which calls are cancellation points. It may refine these
words without reversing their operand order.

### 10. Resources and unsafe regions are visible

The resource-scope candidate consumes a handle and binding name before `with`:

```ricochet
"data/report.txt" fs_open_read try file with
  $file read_all try
end
```

Leaving the block consumes or deterministically closes the affine handle on
success, result propagation, cancellation, and panic unwinding. Explicit close
remains available when its error must be handled:

```ricochet
$file close try
```

Unsafe operations are lexically obvious and contribute the `unsafe` effect:

```ricochet
[
  $buffer pin pinned with
    $pinned address.get foreign_write
  end
] unsafe
```

ADR-003 owns resource and pinning semantics. ADR-005 owns capability grants.
This ADR requires their source forms to expose, not disguise, the boundary.

### 11. Type syntax is confined to declarative islands

Type expressions may use `Name<T>`, tuples, callable signatures, and explicitly
approved scoped punctuation. They appear in:

- callable signatures;
- field and variant declarations;
- explicit local annotations;
- trait bounds and implementations;
- module interfaces and exports;
- FFI declarations and layout blocks; and
- schema declarations.

Type expressions do not make `a + b`, `object.method()`, or `.selector`
executable syntax valid. A generic call normally infers its type arguments. An
explicit type application, if required, must remain a metadata operand before
the called word and pass a separate postfix-vibe review.

### 12. Formatting is part of the grammar contract

The canonical formatter will:

- use two spaces per block level and lowercase `end`;
- keep the terminal declaration word on the header line;
- order modifiers canonically;
- place one blank line between top-level declarations;
- keep short signatures on one line and expand long signatures one input,
  output, bound, or effect per line;
- indent `when` arms one level under `match`;
- preserve comments and source trivia through the lossless CST;
- never reorder executable tokens; and
- be idempotent.

Generated documentation shows the source signature and the fully elaborated
stack/effect signature. The LSP, formatter, docs generator, and compiler all
consume the same CST/HIR rather than reparsing declaration strings.

## Postfix-vibe review

The proposal passes the initial paper review for these reasons:

1. Values and arguments are pushed before the operation that consumes them.
2. Receivers remain immediately before selectors once their arguments are on
   the stack.
3. Declaration headers also terminate in the declaring operation.
4. `match`, `if`, `await`, `try`, `with`, and `unsafe` consume a value or block
   already to their left.
5. Type punctuation is contained in non-executable metadata.
6. Multiword runtime names remain underscore-separated.

The paper review is not acceptance. In particular, generics, nested patterns,
modifier ordering, and type-body capitalization still need realistic formatted
corpora and user testing.

## Diagnostics and tooling effects

The parser emits distinct CST nodes for signatures, generic clauses,
declaration modifiers, types, patterns, effect lists, and executable words.
This enables precise spans rather than treating a signature as a comment.

Diagnostic namespaces are reserved by responsibility:

- `P` for parse and declaration-shape errors;
- `N` for names, modules, and visibility;
- `T` for value types, generics, and traits;
- `S` for stack effects and control-flow joins;
- `E` for effects and capabilities; and
- `R` for affine resources and unsafe lifetime boundaries.

Exact numeric codes are assigned by the accepted diagnostic registry. Codes
remain stable after the first alpha even if wording improves.

For malformed declaration heads, diagnostics show the canonical postfix form.
For stack/type errors, tooling renders both the compact source signature and an
expanded before/after stack. Hover information includes inferred type,
complete stack effect, effect set, visibility, and generic substitutions.

Rename and formatting operate on syntax/name identities, never broad string
replacement. Pattern bindings, field names, variant names, and runtime strings
remain distinguishable to tooling.

## Alternatives rejected at proposal time

### Prefix declarations such as `fn add(left: Int) -> Int`

Rejected because the declaration's controlling word and function name precede
their operands, creating a second language grammar and weakening Ricochet's
postfix identity.

### Infix executable operators with postfix available as an option

Rejected because two evaluation notations double teaching, formatting, macro,
and diagnostic complexity. Ricochet 2 is deliberately postfix rather than a
mainstream syntax with a stack DSL embedded in it.

### Types as comments

Rejected because comments cannot provide a sound public interface, stable
spans, checked effects, generic constraints, or compiler-readable dependency
metadata.

### Dynamic code unless annotated

Rejected by the product charter. Checked code is the default; `Dynamic` and
`unsafe` are explicit boundaries.

### A `!` separator for effects

Not selected for the prototype baseline. It is compact but visually collides
with historically confusing bang-shaped word conventions. `uses` is longer
and immediately searchable. The prototype may include `!` only as a measured
comparison, not an undocumented alias.

### Leading-dot selectors or namespace-dot host APIs

Rejected because they invert the established receiver-selector relationship
and make foreign APIs look unlike ordinary Ricochet code.

### Multiple spellings accepted and normalized later

Rejected because early aliases become compatibility obligations and complicate
formatter/LSP behavior. Prototypes may compare alternatives; the accepted
grammar chooses one canonical form before alpha.

## Prototype evidence

The first preserved Ricochet 2 evidence now lives in
`prototypes/adr-001-surface`. It is an isolated `0.0.0`, non-publishable Rust
workspace member and deliberately does not reuse or modify the Ricochet 1
parser. Its `ricochet2-surface-proof demo` command exercises a valid corpus and
a deliberately invalid corpus through the same lexer, CST, parser, formatter,
and diagnostic pipeline.

The 2026-08-08 evidence run establishes:

- byte-for-byte lossless CST recovery and token-identity round trips, including
  retained comment and whitespace tokens;
- an AST for all 27 Ricochet examples currently fenced in this ADR, with typed
  declarations, bindings, control structures, match arms, and generic postfix
  expressions represented explicitly;
- idempotent formatting for every ADR example and the consolidated proof
  corpus;
- semantic lowering of callable signatures to visible input/output stack rows
  and effect sets;
- five deliberate failures with stable diagnostic codes, line/column
  locations, and exact byte spans; and
- 4,000 deterministic source mutations passing lexer, parser, CST recovery,
  and formatting without a panic.

The executable report is `prototypes/adr-001-surface/PROOF.html`, and CI runs
the crate on Windows, Linux, and macOS because it is a workspace member. This
is evidence for the proposal, not the production Ricochet 2 frontend and not a
compatibility promise.

This first slice does **not** finish the acceptance program. Module-specific
syntax, broader recovery/property fuzzing, LSP fixtures, the independent
readability comparison, and an independent implementation of the Phase 1 proof
CLI remain open. ADR-001 therefore remains Proposed.

ADR-001 cannot become Accepted until a throwaway prototype provides:

1. a lossless CST and semantic AST for every example in this ADR;
2. parse/print round trips preserving comments and token identity;
3. formatter idempotence across a corpus covering declarations, generics,
   nested types, matches, async, resources, OOP, modules, and FFI;
4. deliberate invalid examples with precise stable diagnostic spans;
5. ambiguity and error-recovery fuzzing with no parser crashes;
6. a typed lowering sketch proving signatures can represent the stack rows in
   ADR-002 without hidden source rules;
7. LSP outline, hover, completion, rename, and semantic-token fixtures;
8. a readability review of the capitalized OOP meta-word rule and `uses`
   versus the best effect-separator alternative; and
9. an independent user implementing the Phase 1 proof CLI from public syntax
   documentation alone.

The prototype source and results are preserved even if rejected. Evidence must
record confusing forms and diagnostic failures, not only successful parses.

## Acceptance criteria

Accept this ADR only when:

- every construct has one canonical parse and format;
- ordinary executable examples remain unambiguously postfix;
- the syntax represents all Phase 1 through Phase 6 semantic requirements
  without private parser escapes;
- ADR-002 can assign sound value and stack types to the forms;
- ADR-003, ADR-005, and ADR-006 can expose cleanup, effects, and suspension;
- ADR-004 can express records, enums, traits, classes, and dispatch;
- ADR-008 can emit and consume public signatures in module interfaces;
- diagnostics recover well enough for an editor after an incomplete header or
  match arm; and
- the owner approves the formatted corpus and independent proof report.

If the prototype requires prefix executable syntax, invisible stack shuffling,
or declaration-specific runtime exceptions, this proposal is rejected rather
than patched with aliases. A superseding ADR must preserve the evidence and
explain the revised postfix rule.

## Consequences if accepted

- Ricochet keeps a recognizable postfix identity even with a broad static type
  system.
- Public APIs become compiler-readable and documentation-ready.
- Declaration parsing is richer than 1.x and must be designed as real syntax,
  not implemented by scanning words opportunistically.
- `$` remains meaningful as a lexical read marker while bare identifiers can
  serve as declaration and binding names.
- The formatter becomes mandatory infrastructure early in Phase 1.
- Some compact mainstream idioms are intentionally unavailable when they
  compromise stack order or make declarations prefix-shaped.
