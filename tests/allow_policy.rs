//! Executable contract for the first-party Rust source policy: suppression
//! attributes, expectation scope, assertions outside test code, and workspace
//! lint inheritance.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use serde::Deserialize;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Expr, Lit, Meta, Token};

const ROOT_EXCLUDED_DIRECTORIES: [&str; 4] = ["mutants.out", "mutants.out.old", "target", "vendor"];

/// The lint groups of the pinned toolchain, as `rustc`, `clippy-driver`, and
/// `rustdoc` list them with `-W help`. An expectation on a group silences
/// every lint in it.
const LINT_GROUPS: [&str; 24] = [
    "clippy::all",
    "clippy::cargo",
    "clippy::complexity",
    "clippy::correctness",
    "clippy::nursery",
    "clippy::pedantic",
    "clippy::perf",
    "clippy::restriction",
    "clippy::style",
    "clippy::suspicious",
    "deprecated_safe",
    "future_incompatible",
    "keyword_idents",
    "let_underscore",
    "nonstandard_style",
    "refining_impl_trait",
    "rust_2018_compatibility",
    "rust_2018_idioms",
    "rust_2021_compatibility",
    "rust_2024_compatibility",
    "rustdoc::all",
    "unknown_or_malformed_diagnostic_attributes",
    "unused",
    "warnings",
];

/// The lints that keep `#[allow]` out; expecting them reopens that door.
const ALLOW_ATTRIBUTE_LINTS: [&str; 2] = [
    "clippy::allow_attributes",
    "clippy::allow_attributes_without_reason",
];

/// The Cargo target kind whose roots may carry a crate-level expectation.
/// Benches, examples, and build scripts are not test crates.
const TEST_TARGET_KIND: &str = "test";

/// The denied restriction lints whose findings are panic sites in
/// expressions. A module-wide expectation of one silences every such site in
/// the module.
const PANIC_LINTS: [&str; 10] = [
    "clippy::arithmetic_side_effects",
    "clippy::exit",
    "clippy::expect_used",
    "clippy::indexing_slicing",
    "clippy::panic",
    "clippy::string_slice",
    "clippy::todo",
    "clippy::unimplemented",
    "clippy::unreachable",
    "clippy::unwrap_used",
];

/// The other Clippy lints the workspace denies by name. Their findings are not
/// panic sites, so a module-wide expectation of one stays allowed. Every lint
/// denied by name sits in this list, `PANIC_LINTS`, or `ALLOW_ATTRIBUTE_LINTS`,
/// so denying a new one forces that classification.
const NON_PANIC_LINTS: [&str; 10] = [
    "clippy::clone_on_ref_ptr",
    "clippy::dbg_macro",
    "clippy::let_underscore_must_use",
    "clippy::map_err_ignore",
    "clippy::mem_forget",
    "clippy::print_stderr",
    "clippy::print_stdout",
    "clippy::rc_buffer",
    "clippy::unused_result_ok",
    "clippy::wildcard_dependencies",
];

/// Assertion macros that stay in release builds; `debug_assert!` and its
/// variants are not among them.
const ASSERTION_MACROS: [&str; 3] = ["assert", "assert_eq", "assert_ne"];

/// Keywords that start an item or a `let` statement, whose generic arguments
/// may hold commas. `mod`, `const`, and `static` have their own handling.
const ITEM_KEYWORDS: [&str; 9] = [
    "enum", "extern", "fn", "impl", "let", "struct", "trait", "type", "use",
];

#[derive(Deserialize)]
struct CargoMetadataDocument {
    packages: Vec<CargoPackage>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct CargoPackage {
    id: String,
    manifest_path: PathBuf,
    targets: Vec<CargoTarget>,
}

#[derive(Deserialize)]
struct CargoTarget {
    kind: Vec<String>,
    src_path: PathBuf,
}

#[derive(Default)]
struct CargoWorkspaceInputs {
    enumeration_roots: BTreeSet<PathBuf>,
    package_roots: BTreeSet<PathBuf>,
    target_sources: Vec<PathBuf>,
    non_test_crate_roots: BTreeSet<PathBuf>,
    test_crate_roots: BTreeSet<PathBuf>,
}

/// What the module walk knows about the code at one token level.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CodeContext {
    /// Inside a Cargo test target, an item under a `cfg` that requires `test`,
    /// or a function with a `test` attribute.
    test: bool,
    /// Inside a `const` or `static` initializer or an inline `const` block,
    /// which runs at compile time.
    compile_time: bool,
}

/// The strictest panic-lint expectation among some attributes.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum PanicExpectation {
    #[default]
    Absent,
    /// Of a panic lint that test code may expect module-wide.
    Panic,
    /// Of `clippy::arithmetic_side_effects`, which test code may not expect.
    Arithmetic,
}

/// What the outer attributes since the last item or statement boundary say.
#[derive(Default)]
struct OuterAttributes {
    test: bool,
    /// A `path` attribute, which the source-reference check rejects.
    custom_path: bool,
    expectation: PanicExpectation,
}

/// How much of an item or statement the walk has read.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ItemKind {
    /// No item keyword yet, as in a match arm, a field, or an element, which
    /// a comma ends.
    #[default]
    Unknown,
    /// An item or `let` statement, whose generic arguments may hold commas.
    Item,
    /// A `const` or `static` item before its `=`.
    Constant,
    /// A `const` or `static` item's initializer.
    Initializer,
}

/// What the walk knows about the code since the last item or statement
/// boundary.
#[derive(Default)]
struct ItemState {
    attributes: OuterAttributes,
    kind: ItemKind,
    /// A closure, function, `async` block, or macro definition inside
    /// compile-time code, whose body can run at run time.
    run_time: bool,
    /// An `impl` or `trait` item, whose body spans many functions as a
    /// module does.
    block: bool,
}

/// A source file in a module tree, and the directory where its `mod name;`
/// declarations find `name.rs` or `name/mod.rs`.
struct ModuleSource {
    path: PathBuf,
    child_directory: PathBuf,
}

/// One token level of the module walk.
struct TokenLevel<'walk> {
    trees: Vec<TokenTree>,
    source: &'walk ModuleSource,
    context: CodeContext,
    /// A module body, whose inner attributes apply to the whole module. The
    /// crate root's top level is not one, because the crate-level check
    /// covers it.
    module_scope: bool,
    item: ItemState,
}

/// An attribute and the index of its bracket group.
struct Attribute {
    tokens: TokenStream,
    meta: Option<Meta>,
    inner: bool,
    end: usize,
}

/// Findings that need the module tree to tell test code apart, counted per
/// file.
#[derive(Default)]
struct ModuleFindings {
    /// Each walked file with its context.
    visited: BTreeSet<(PathBuf, CodeContext)>,
    module_expectations: BTreeMap<PathBuf, usize>,
    assertions: BTreeMap<PathBuf, usize>,
    unresolved_modules: BTreeMap<PathBuf, usize>,
}

fn allow_policy_violation_count(source: &str) -> Result<usize, proc_macro2::LexError> {
    TokenStream::from_str(source).map(|tokens| allow_violations_in_tokens(&tokens))
}

fn allow_violations_in_tokens(tokens: &TokenStream) -> usize {
    let token_trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut count = 0_usize;

    for (index, token) in token_trees.iter().enumerate() {
        if let TokenTree::Group(group) = token {
            count = count.saturating_add(allow_violations_in_tokens(&group.stream()));
        }

        let Some(attribute) = attribute_at(&token_trees, index) else {
            continue;
        };
        let attribute_tokens: Vec<_> = attribute.tokens.clone().into_iter().collect();
        let fixed_doc_value = matches!(
            attribute_tokens.as_slice(),
            [TokenTree::Ident(name), TokenTree::Punct(equals), ..]
                if ident_is_name(name, "doc") && equals.as_char() == '='
        );
        if (contains_macro_metavariable(&attribute.tokens) && !fixed_doc_value)
            || attribute.meta.is_some_and(|meta| {
                meta_contains(&meta, |meta| {
                    path_is_ident(meta.path(), "allow") || is_broad_expectation(meta)
                })
            })
        {
            count = count.saturating_add(1);
        }
    }

    count
}

fn contains_macro_metavariable(tokens: &TokenStream) -> bool {
    tokens.clone().into_iter().any(|token| match token {
        TokenTree::Punct(punctuation) => punctuation.as_char() == '$',
        TokenTree::Group(group) => contains_macro_metavariable(&group.stream()),
        _ => false,
    })
}

enum SourceReference {
    LiteralInclude(String),
    OpaqueInclude,
    CustomModulePath,
}

fn source_references(tokens: &TokenStream) -> Vec<SourceReference> {
    let token_trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut references = Vec::new();

    for (index, token) in token_trees.iter().enumerate() {
        if let TokenTree::Group(group) = token {
            references.extend(source_references(&group.stream()));
        }

        let previous = index
            .checked_sub(1)
            .and_then(|previous| token_trees.get(previous));
        if let TokenTree::Ident(identifier) = token
            && ident_is_name(identifier, "include")
            && is_punct(token_trees.get(index.saturating_add(1)), '!')
            && !is_punct(previous, '$')
        {
            references.push(match token_trees.get(index.saturating_add(2)) {
                Some(TokenTree::Group(arguments)) => literal_include_reference(arguments),
                _ => SourceReference::OpaqueInclude,
            });
        }

        if attribute_at(&token_trees, index)
            .and_then(|attribute| attribute.meta)
            .is_some_and(|meta| meta_contains(&meta, is_path_attribute))
        {
            references.push(SourceReference::CustomModulePath);
        }
    }

    references
}

fn literal_include_reference(arguments: &proc_macro2::Group) -> SourceReference {
    let Ok(expressions) =
        Punctuated::<Expr, Token![,]>::parse_terminated.parse2(arguments.stream())
    else {
        return SourceReference::OpaqueInclude;
    };
    if expressions.len() == 1
        && let Some(Expr::Lit(expression)) = expressions.first()
        && let Lit::Str(path) = &expression.lit
    {
        return SourceReference::LiteralInclude(path.value());
    }
    SourceReference::OpaqueInclude
}

fn ident_is_name(identifier: &proc_macro2::Ident, expected: &str) -> bool {
    let identifier = identifier.to_string();
    identifier.strip_prefix("r#").unwrap_or(&identifier) == expected
}

/// Matches an attribute directly or nested at any depth in `cfg_attr`.
fn meta_contains(meta: &Meta, matches: fn(&Meta) -> bool) -> bool {
    if matches(meta) {
        return true;
    }
    let Meta::List(list) = meta else {
        return false;
    };
    if !path_is_ident(&list.path, "cfg_attr") {
        return false;
    }

    // Skip the predicate as raw tokens: `true` and `false` are not `Meta`.
    let attributes: TokenStream = list
        .tokens
        .clone()
        .into_iter()
        .skip_while(|token| !matches!(token, TokenTree::Punct(comma) if comma.as_char() == ','))
        .skip(1)
        .collect();
    Punctuated::<Meta, Token![,]>::parse_terminated
        .parse2(attributes)
        .is_ok_and(|metas| metas.iter().any(|meta| meta_contains(meta, matches)))
}

/// An expectation must name one lint that is neither a group nor a lint that
/// keeps `#[allow]` out.
fn is_broad_expectation(meta: &Meta) -> bool {
    let lints = expected_lints(meta);
    lints.len() > 1
        || lints.iter().any(|lint| {
            LINT_GROUPS.contains(&lint.as_str()) || ALLOW_ATTRIBUTE_LINTS.contains(&lint.as_str())
        })
}

fn expects_arithmetic(meta: &Meta) -> bool {
    expected_lints(meta)
        .iter()
        .any(|lint| lint == "clippy::arithmetic_side_effects")
}

/// The lints an `expect` attribute names; empty for any other attribute.
fn expected_lints(meta: &Meta) -> Vec<String> {
    let Meta::List(list) = meta else {
        return Vec::new();
    };
    if !path_is_ident(&list.path, "expect") {
        return Vec::new();
    }
    Punctuated::<Meta, Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .map(|arguments| {
            arguments
                .iter()
                .filter_map(|argument| match argument {
                    Meta::Path(path) => Some(lint_name(path)),
                    Meta::List(_) | Meta::NameValue(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn lint_name(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| {
            let identifier = segment.ident.to_string();
            identifier
                .strip_prefix("r#")
                .unwrap_or(&identifier)
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("::")
}

/// Counts the crate-level `#![expect]` attributes of a crate root, direct or
/// nested in `cfg_attr`, that `matches` accepts. Inner attributes inside
/// modules and items are deeper in the token tree and stay out of the count.
fn crate_level_expectation_count(tokens: &TokenStream, matches: fn(&Meta) -> bool) -> usize {
    let token_trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    (0..token_trees.len())
        .filter_map(|index| attribute_at(&token_trees, index))
        .filter(|attribute| {
            attribute.inner
                && attribute
                    .meta
                    .as_ref()
                    .is_some_and(|meta| meta_contains(meta, matches))
        })
        .count()
}

fn is_expectation(meta: &Meta) -> bool {
    path_is_ident(meta.path(), "expect")
}

fn expects_panic_lint(meta: &Meta) -> bool {
    expected_lints(meta)
        .iter()
        .any(|lint| PANIC_LINTS.contains(&lint.as_str()))
}

/// `#[test]`, a test attribute such as `#[tokio::test]`, or a `cfg` that
/// requires `test`.
fn marks_test(meta: &Meta) -> bool {
    if meta
        .path()
        .segments
        .last()
        .is_some_and(|segment| ident_is_name(&segment.ident, "test"))
    {
        return true;
    }
    let Meta::List(list) = meta else {
        return false;
    };
    path_is_ident(&list.path, "cfg")
        && syn::parse2::<Meta>(list.tokens.clone())
            .is_ok_and(|predicate| cfg_requires_test(&predicate))
}

fn cfg_requires_test(predicate: &Meta) -> bool {
    match predicate {
        Meta::Path(path) => path_is_ident(path, "test"),
        Meta::List(list) if path_is_ident(&list.path, "all") => {
            Punctuated::<Meta, Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .is_ok_and(|predicates| predicates.iter().any(cfg_requires_test))
        },
        Meta::List(_) | Meta::NameValue(_) => false,
    }
}

fn count_in(counts: &mut BTreeMap<PathBuf, usize>, path: &Path) {
    let count = counts.entry(path.to_path_buf()).or_default();
    *count = count.saturating_add(1);
}

impl PanicExpectation {
    fn of(meta: &Meta) -> Self {
        if meta_contains(meta, expects_arithmetic) {
            Self::Arithmetic
        } else if meta_contains(meta, expects_panic_lint) {
            Self::Panic
        } else {
            Self::Absent
        }
    }

    /// Whether the expectation is too broad for a whole module.
    fn forbidden_module_wide(self, test: bool) -> bool {
        match self {
            Self::Absent => false,
            Self::Panic => !test,
            Self::Arithmetic => true,
        }
    }
}

/// Walks one crate's module tree from its root.
fn walk_crate(crate_root: &Path, test: bool, findings: &mut ModuleFindings) -> io::Result<()> {
    let Some(directory) = crate_root.parent() else {
        return Ok(());
    };
    let source = ModuleSource {
        path: crate_root.to_path_buf(),
        child_directory: directory.to_path_buf(),
    };
    let context = CodeContext {
        test,
        compile_time: false,
    };
    walk_module_file(&source, context, false, findings)
}

fn walk_module_file(
    source: &ModuleSource,
    context: CodeContext,
    module_scope: bool,
    findings: &mut ModuleFindings,
) -> io::Result<()> {
    if !findings
        .visited
        .insert((fs::canonicalize(&source.path)?, context))
    {
        return Ok(());
    }
    // scan_source_file reports a file that does not tokenize.
    let Ok(tokens) = TokenStream::from_str(&fs::read_to_string(&source.path)?) else {
        return Ok(());
    };
    walk_tokens(&tokens, source, context, module_scope, findings)
}

/// Walks one token level, whose leading inner attributes apply to all of it.
fn walk_tokens(
    tokens: &TokenStream,
    source: &ModuleSource,
    context: CodeContext,
    module_scope: bool,
    findings: &mut ModuleFindings,
) -> io::Result<()> {
    let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let test = context.test || leading_inner_attributes(&trees).iter().any(marks_test);
    let mut level = TokenLevel {
        trees,
        source,
        context: CodeContext { test, ..context },
        module_scope,
        item: ItemState::default(),
    };
    level.walk(findings)
}

fn is_punct(token: Option<&TokenTree>, character: char) -> bool {
    matches!(token, Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == character)
}

/// The `!` of a macro call, which `!=` is not.
fn is_macro_bang(token: Option<&TokenTree>) -> bool {
    matches!(token, Some(TokenTree::Punct(bang)) if bang.as_char() == '!' && bang.spacing() == Spacing::Alone)
}

fn is_path_attribute(meta: &Meta) -> bool {
    path_is_ident(meta.path(), "path")
}

fn is_assertion_macro(identifier: &proc_macro2::Ident) -> bool {
    ASSERTION_MACROS
        .iter()
        .any(|name| ident_is_name(identifier, name))
}

fn attribute_at(trees: &[TokenTree], index: usize) -> Option<Attribute> {
    if !is_punct(trees.get(index), '#') {
        return None;
    }
    let inner = is_punct(trees.get(index.saturating_add(1)), '!');
    let end = index.saturating_add(if inner { 2 } else { 1 });
    match trees.get(end) {
        Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Bracket => {
            Some(Attribute {
                tokens: group.stream(),
                meta: syn::parse2(group.stream()).ok(),
                inner,
                end,
            })
        },
        _ => None,
    }
}

fn leading_inner_attributes(trees: &[TokenTree]) -> Vec<Meta> {
    let mut metas = Vec::new();
    let mut index = 0_usize;
    while let Some(attribute) = attribute_at(trees, index)
        && attribute.inner
    {
        metas.extend(attribute.meta);
        index = attribute.end.saturating_add(1);
    }
    metas
}

impl TokenLevel<'_> {
    fn walk(&mut self, findings: &mut ModuleFindings) -> io::Result<()> {
        let mut index = 0_usize;
        while let Some(token) = self.trees.get(index).cloned() {
            let consumed = match &token {
                TokenTree::Punct(punctuation) => {
                    self.punctuation(index, punctuation.as_char(), findings)
                },
                TokenTree::Ident(identifier) => self.identifier(index, identifier, findings)?,
                TokenTree::Group(group) => {
                    let block_body = group.delimiter() == Delimiter::Brace && self.item.block;
                    walk_tokens(
                        &group.stream(),
                        self.source,
                        self.here(),
                        block_body,
                        findings,
                    )?;
                    // A brace group ends an item, except inside a constant.
                    if group.delimiter() == Delimiter::Brace
                        && !matches!(self.item.kind, ItemKind::Constant | ItemKind::Initializer)
                    {
                        self.item = ItemState::default();
                    }
                    0
                },
                TokenTree::Literal(_) => 0,
            };
            index = index.saturating_add(consumed).saturating_add(1);
        }
        Ok(())
    }

    fn here(&self) -> CodeContext {
        CodeContext {
            test: self.context.test || self.item.attributes.test,
            compile_time: (self.context.compile_time || self.item.kind == ItemKind::Initializer)
                && !self.item.run_time,
        }
    }

    fn previous(&self, index: usize) -> Option<&TokenTree> {
        index
            .checked_sub(1)
            .and_then(|previous| self.trees.get(previous))
    }

    /// Handles punctuation and returns how many further tokens it consumed.
    fn punctuation(
        &mut self,
        index: usize,
        character: char,
        findings: &mut ModuleFindings,
    ) -> usize {
        match character {
            '#' => self.attribute(index, findings),
            ';' => {
                self.item = ItemState::default();
                0
            },
            ',' if self.item.kind == ItemKind::Unknown => {
                self.item = ItemState::default();
                0
            },
            '=' if self.item.kind == ItemKind::Constant => {
                self.item.kind = ItemKind::Initializer;
                0
            },
            '|' if self.here().compile_time && self.starts_closure(index) => {
                self.item.run_time = true;
                0
            },
            _ => 0,
        }
    }

    /// A `|` that opens closure parameters. An operator or pattern alternative
    /// follows an operand instead.
    fn starts_closure(&self, index: usize) -> bool {
        match self.previous(index) {
            None => true,
            Some(TokenTree::Punct(previous)) => !['|', '?'].contains(&previous.as_char()),
            Some(TokenTree::Ident(previous)) => previous == "move" || previous == "return",
            Some(TokenTree::Group(_) | TokenTree::Literal(_)) => false,
        }
    }

    /// Handles the attribute whose `#` is at `index` and returns how many
    /// further tokens it consumed.
    fn attribute(&mut self, index: usize, findings: &mut ModuleFindings) -> usize {
        let Some(attribute) = attribute_at(&self.trees, index) else {
            return 0;
        };
        let consumed = attribute.end.saturating_sub(index);
        let Some(meta) = &attribute.meta else {
            return consumed;
        };
        let expectation = PanicExpectation::of(meta);
        if attribute.inner {
            if self.module_scope && expectation.forbidden_module_wide(self.context.test) {
                count_in(&mut findings.module_expectations, &self.source.path);
            }
        } else {
            let attributes = &mut self.item.attributes;
            attributes.test |= marks_test(meta);
            attributes.custom_path |= meta_contains(meta, is_path_attribute);
            attributes.expectation = attributes.expectation.max(expectation);
        }
        consumed
    }

    /// Handles an identifier and returns how many further tokens it consumed.
    fn identifier(
        &mut self,
        index: usize,
        identifier: &proc_macro2::Ident,
        findings: &mut ModuleFindings,
    ) -> io::Result<usize> {
        let next = self.trees.get(index.saturating_add(1));
        if is_macro_bang(next) && !is_punct(self.previous(index), '$') {
            self.macro_call(index, identifier, findings)?;
            return Ok(0);
        }
        // An import that renames one, as in `use std::assert as check;`.
        if is_assertion_macro(identifier)
            && matches!(next, Some(TokenTree::Ident(rename)) if rename == "as")
        {
            self.count_assertion(findings);
        }
        // Keywords only: a raw identifier such as `r#mod` is not one.
        let keyword = identifier.to_string();
        match keyword.as_str() {
            "mod" => return self.module(index, findings),
            "const" | "static" => return self.constant(index, findings),
            // Their bodies can run at run time even inside compile-time code.
            "fn" | "async" => self.item.run_time |= self.here().compile_time,
            _ => {},
        }
        // After another keyword, `impl` names a type, as in `-> impl Trait`.
        if ["impl", "trait"].contains(&keyword.as_str()) && self.item.kind == ItemKind::Unknown {
            self.item.block = true;
            if self
                .item
                .attributes
                .expectation
                .forbidden_module_wide(self.here().test)
            {
                count_in(&mut findings.module_expectations, &self.source.path);
            }
        }
        if ITEM_KEYWORDS.contains(&keyword.as_str()) && self.item.kind == ItemKind::Unknown {
            self.item.kind = ItemKind::Item;
        }
        Ok(0)
    }

    fn macro_call(
        &mut self,
        index: usize,
        identifier: &proc_macro2::Ident,
        findings: &mut ModuleFindings,
    ) -> io::Result<()> {
        if ident_is_name(identifier, "include") {
            return self.include(index, findings);
        }
        if ident_is_name(identifier, "macro_rules") {
            // A macro body runs wherever the macro expands.
            self.item.run_time |= self.here().compile_time;
        } else if is_assertion_macro(identifier) {
            self.count_assertion(findings);
        }
        Ok(())
    }

    fn count_assertion(&self, findings: &mut ModuleFindings) {
        let here = self.here();
        if !here.test && !here.compile_time {
            count_in(&mut findings.assertions, &self.source.path);
        }
    }

    /// Walks a literal `include!` input in place of the call.
    fn include(&self, index: usize, findings: &mut ModuleFindings) -> io::Result<()> {
        if let Some(TokenTree::Group(arguments)) = self.trees.get(index.saturating_add(2))
            && let SourceReference::LiteralInclude(relative) = literal_include_reference(arguments)
            && let Some(directory) = self.source.path.parent()
        {
            let included = ModuleSource {
                path: directory.join(relative),
                child_directory: self.source.child_directory.clone(),
            };
            if included.path.is_file() {
                walk_module_file(&included, self.here(), self.module_scope, findings)?;
            }
        }
        Ok(())
    }

    /// Handles `const` or `static` and returns how many further tokens it
    /// consumed. An inline `const` block and an item's initializer run at
    /// compile time; a `const fn` does not.
    fn constant(&mut self, index: usize, findings: &mut ModuleFindings) -> io::Result<usize> {
        let next = self.trees.get(index.saturating_add(1));
        if let Some(TokenTree::Group(block)) = next
            && block.delimiter() == Delimiter::Brace
        {
            let context = CodeContext {
                compile_time: true,
                ..self.here()
            };
            walk_tokens(&block.stream(), self.source, context, false, findings)?;
            return Ok(1);
        }
        // `*const`, `&raw const`, `'static`, and const generic parameters.
        let follows_raw =
            matches!(self.previous(index), Some(TokenTree::Ident(previous)) if previous == "raw");
        let follows_punctuation = ['*', '<', ',', '\'']
            .into_iter()
            .any(|character| is_punct(self.previous(index), character));
        let qualifies_function = matches!(next, Some(TokenTree::Ident(following))
            if ["fn", "unsafe", "async", "extern"].contains(&following.to_string().as_str()));
        if !follows_raw && !follows_punctuation && !qualifies_function {
            self.item.kind = ItemKind::Constant;
        }
        Ok(0)
    }

    /// Handles `mod` and returns how many further tokens it consumed. The
    /// module's inline body or file takes the declaration's test context.
    fn module(&mut self, index: usize, findings: &mut ModuleFindings) -> io::Result<usize> {
        let context = CodeContext {
            test: self.here().test,
            compile_time: false,
        };
        let attributes = mem::take(&mut self.item.attributes);
        if attributes.expectation.forbidden_module_wide(context.test) {
            count_in(&mut findings.module_expectations, &self.source.path);
        }
        // A macro can name the module with a metavariable, whose file the
        // walk cannot find.
        let after = |offset: usize| index.saturating_add(offset);
        let (name, body_index) = match (self.trees.get(after(1)), self.trees.get(after(2))) {
            (Some(TokenTree::Ident(name)), _) => (name.to_string(), after(2)),
            (Some(TokenTree::Punct(dollar)), Some(TokenTree::Ident(name)))
                if dollar.as_char() == '$' =>
            {
                (format!("${name}"), after(3))
            },
            _ => return Ok(0),
        };
        let name = name.strip_prefix("r#").unwrap_or(&name);
        let child_directory = self.source.child_directory.join(name);
        match self.trees.get(body_index) {
            Some(TokenTree::Group(body)) if body.delimiter() == Delimiter::Brace => {
                let inline = ModuleSource {
                    path: self.source.path.clone(),
                    child_directory,
                };
                walk_tokens(&body.stream(), &inline, context, true, findings)?;
                self.item = ItemState::default();
                Ok(body_index.saturating_sub(index))
            },
            Some(TokenTree::Punct(semicolon)) if semicolon.as_char() == ';' => {
                // The source-reference check rejects a `path` attribute.
                if !attributes.custom_path {
                    self.module_file(name, child_directory, context, findings)?;
                }
                Ok(body_index.saturating_sub(index).saturating_sub(1))
            },
            _ => Ok(0),
        }
    }

    /// Walks the file of `mod name;`, which must exist as `name.rs` or
    /// `name/mod.rs`.
    fn module_file(
        &self,
        name: &str,
        child_directory: PathBuf,
        context: CodeContext,
        findings: &mut ModuleFindings,
    ) -> io::Result<()> {
        let files: Vec<PathBuf> = [
            self.source.child_directory.join(format!("{name}.rs")),
            child_directory.join("mod.rs"),
        ]
        .into_iter()
        .filter(|path| path.is_file())
        .collect();
        if files.is_empty() {
            count_in(&mut findings.unresolved_modules, &self.source.path);
        }
        for path in files {
            let file = ModuleSource {
                path,
                child_directory: child_directory.clone(),
            };
            walk_module_file(&file, context, true, findings)?;
        }
        Ok(())
    }
}

fn path_is_ident(path: &syn::Path, expected: &str) -> bool {
    path.leading_colon.is_none()
        && path.segments.len() == 1
        && path
            .segments
            .first()
            .is_some_and(|segment| ident_is_name(&segment.ident, expected))
}

fn collect_project_files(
    path: &Path,
    canonical_root: &Path,
    package_roots: &BTreeSet<PathBuf>,
    visited_directories: &mut BTreeSet<PathBuf>,
    source_files: &mut Vec<PathBuf>,
    manifests: &mut Vec<PathBuf>,
    violations: &mut Vec<String>,
) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        let canonical_path = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) => {
                violations.push(format!("{} cannot be resolved: {error}", path.display()));
                return Ok(());
            },
        };
        let target_metadata = fs::metadata(path)?;
        if target_metadata.is_dir() {
            if !canonical_path.starts_with(canonical_root) {
                violations.push(format!(
                    "{} resolves outside canonical project root {}",
                    path.display(),
                    canonical_root.display()
                ));
                return Ok(());
            }
            if !visited_directories.insert(canonical_path) {
                return Ok(());
            }
            for entry in fs::read_dir(path)? {
                collect_project_files(
                    &entry?.path(),
                    canonical_root,
                    package_roots,
                    visited_directories,
                    source_files,
                    manifests,
                    violations,
                )?;
            }
            return Ok(());
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            source_files.push(path.to_path_buf());
        }
        if is_cargo_manifest(path) {
            manifests.push(path.to_path_buf());
        }
        return Ok(());
    }
    if metadata.is_file() {
        if path.extension().is_some_and(|extension| extension == "rs") {
            source_files.push(path.to_path_buf());
        }
        if is_cargo_manifest(path) {
            manifests.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !metadata.is_dir() || is_excluded_directory(path, package_roots)? {
        return Ok(());
    }
    if !visited_directories.insert(fs::canonicalize(path)?) {
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        collect_project_files(
            &entry?.path(),
            canonical_root,
            package_roots,
            visited_directories,
            source_files,
            manifests,
            violations,
        )?;
    }
    Ok(())
}

fn is_excluded_directory(path: &Path, package_roots: &BTreeSet<PathBuf>) -> io::Result<bool> {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(false);
    };
    if name == ".git" {
        return Ok(true);
    }
    if !ROOT_EXCLUDED_DIRECTORIES.contains(&name) {
        return Ok(false);
    }
    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    Ok(package_roots.contains(&fs::canonicalize(parent)?))
}

fn is_cargo_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "Cargo.toml")
}

fn project_policy_violations(root: &Path) -> io::Result<Vec<String>> {
    let canonical_root = fs::canonicalize(root)?;
    let mut source_files = Vec::new();
    let mut manifests = Vec::new();
    let mut visited_directories = BTreeSet::new();
    let mut violations = Vec::new();
    let CargoWorkspaceInputs {
        mut enumeration_roots,
        mut package_roots,
        target_sources,
        non_test_crate_roots,
        test_crate_roots,
    } = cargo_workspace_inputs(root, &canonical_root, &mut violations);
    package_roots.insert(canonical_root.clone());
    enumeration_roots.insert(canonical_root.clone());
    collect_project_files(
        root,
        &canonical_root,
        &package_roots,
        &mut visited_directories,
        &mut source_files,
        &mut manifests,
        &mut violations,
    )?;
    for enumeration_root in enumeration_roots {
        if enumeration_root != canonical_root {
            collect_source_root_files(
                &enumeration_root,
                &canonical_root,
                &package_roots,
                &mut visited_directories,
                &mut source_files,
                &mut manifests,
                &mut violations,
            )?;
        }
    }
    source_files.extend(target_sources);
    manifests.sort();
    for manifest in manifests {
        source_files.extend(cargo_target_source_paths(
            &manifest,
            &canonical_root,
            &mut violations,
        )?);
    }
    source_files.sort();

    let mut visited = BTreeSet::new();
    for source_file in source_files {
        scan_source_file(&source_file, &canonical_root, &mut visited, &mut violations)?;
    }
    for crate_root in &non_test_crate_roots {
        // scan_source_file reports a crate root that does not tokenize.
        let Ok(tokens) = TokenStream::from_str(&fs::read_to_string(crate_root)?) else {
            continue;
        };
        let count = crate_level_expectation_count(&tokens, is_expectation);
        if count != 0 {
            violations.push(format!(
                "{} has crate-level #![expect] attributes ({count}); outside test targets, put each expectation on the smallest enclosing statement or item",
                crate_root.display()
            ));
        }
    }
    for crate_root in &test_crate_roots {
        let Ok(tokens) = TokenStream::from_str(&fs::read_to_string(crate_root)?) else {
            continue;
        };
        if crate_level_expectation_count(&tokens, expects_arithmetic) != 0 {
            violations.push(format!(
                "{} has a crate-level arithmetic expectation; test code takes no arithmetic expectation, so use checked_ methods (AGENTS.md)",
                crate_root.display()
            ));
        }
    }
    let mut findings = ModuleFindings::default();
    for crate_root in &non_test_crate_roots {
        walk_crate(crate_root, false, &mut findings)?;
    }
    for crate_root in &test_crate_roots {
        walk_crate(crate_root, true, &mut findings)?;
    }
    for (path, count) in &findings.module_expectations {
        violations.push(format!(
            "{} has module-, impl-, or trait-wide expectations of panic lints ({count}); outside test code, put each on the smallest enclosing statement or item, and use checked_ methods for test arithmetic (AGENTS.md)",
            path.display()
        ));
    }
    for (path, count) in &findings.unresolved_modules {
        violations.push(format!(
            "{} declares modules whose files the policy scanner cannot find ({count}); name each module literally and keep its file at name.rs or name/mod.rs",
            path.display()
        ));
    }
    for (path, count) in &findings.assertions {
        violations.push(format!(
            "{} calls assert!, assert_eq!, or assert_ne! outside test code ({count}); return a typed error instead (AGENTS.md)",
            path.display()
        ));
    }
    violations.sort();
    Ok(violations)
}

fn collect_source_root_files(
    source_root: &Path,
    canonical_root: &Path,
    package_roots: &BTreeSet<PathBuf>,
    visited_directories: &mut BTreeSet<PathBuf>,
    source_files: &mut Vec<PathBuf>,
    manifests: &mut Vec<PathBuf>,
    violations: &mut Vec<String>,
) -> io::Result<()> {
    if !visited_directories.insert(source_root.to_path_buf()) {
        return Ok(());
    }
    for entry in fs::read_dir(source_root)? {
        collect_project_files(
            &entry?.path(),
            canonical_root,
            package_roots,
            visited_directories,
            source_files,
            manifests,
            violations,
        )?;
    }
    Ok(())
}

fn cargo_workspace_inputs(
    root: &Path,
    canonical_root: &Path,
    violations: &mut Vec<String>,
) -> CargoWorkspaceInputs {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.is_file() {
        return CargoWorkspaceInputs::default();
    }

    let output = match Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--locked",
            "--format-version=1",
            "--manifest-path",
        ])
        .arg(&manifest_path)
        .current_dir(root)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            violations.push(format!(
                "cannot run locked Cargo metadata for {}: {error}",
                manifest_path.display()
            ));
            return CargoWorkspaceInputs::default();
        },
    };
    if !output.status.success() {
        violations.push(format!(
            "locked Cargo metadata failed for {}: {}",
            manifest_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
        return CargoWorkspaceInputs::default();
    }

    let metadata = match serde_json::from_slice::<CargoMetadataDocument>(&output.stdout) {
        Ok(metadata) => metadata,
        Err(error) => {
            violations.push(format!(
                "locked Cargo metadata for {} is not valid JSON: {error}",
                manifest_path.display()
            ));
            return CargoWorkspaceInputs::default();
        },
    };
    let mut packages_by_id = BTreeMap::new();
    for package in metadata.packages {
        if packages_by_id.contains_key(&package.id) {
            violations.push(format!(
                "locked Cargo metadata contains duplicate package id {:?}",
                package.id
            ));
            continue;
        }
        packages_by_id.insert(package.id.clone(), package);
    }

    let mut inputs = CargoWorkspaceInputs::default();
    let mut seen_members = BTreeSet::new();
    for member_id in metadata.workspace_members {
        if !seen_members.insert(member_id.clone()) {
            violations.push(format!(
                "locked Cargo metadata contains duplicate workspace member id {member_id:?}"
            ));
            continue;
        }
        let Some(package) = packages_by_id.get(&member_id) else {
            violations.push(format!(
                "locked Cargo metadata workspace member {member_id:?} has no matching package"
            ));
            continue;
        };
        let Some(canonical_manifest) = canonical_workspace_path(
            &package.manifest_path,
            "workspace member manifest",
            canonical_root,
            violations,
        ) else {
            continue;
        };
        let Some(package_root) = canonical_manifest.parent() else {
            violations.push(format!(
                "workspace member manifest {} has no package root",
                canonical_manifest.display()
            ));
            continue;
        };
        match fs::read_to_string(&canonical_manifest)
            .and_then(|source| toml::from_str::<toml::Value>(&source).map_err(io::Error::other))
        {
            Ok(manifest) => {
                let inherits_lints = manifest
                    .get("lints")
                    .and_then(|lints| lints.get("workspace"))
                    .and_then(toml::Value::as_bool)
                    == Some(true);
                if !inherits_lints {
                    violations.push(format!(
                        "{} must declare [lints] workspace = true",
                        canonical_manifest.display()
                    ));
                }
            },
            Err(error) => violations.push(format!(
                "cannot inspect workspace lint inheritance in {}: {error}",
                canonical_manifest.display()
            )),
        }
        inputs.package_roots.insert(package_root.to_path_buf());
        inputs.enumeration_roots.insert(package_root.to_path_buf());
        for target in &package.targets {
            if let Some(source) = canonical_workspace_path(
                &target.src_path,
                "workspace member target",
                canonical_root,
                violations,
            ) {
                if let Some(parent) = source.parent() {
                    inputs.enumeration_roots.insert(parent.to_path_buf());
                }
                if target.kind.iter().any(|kind| kind != TEST_TARGET_KIND) {
                    inputs.non_test_crate_roots.insert(source);
                } else {
                    inputs.test_crate_roots.insert(source);
                }
                inputs.target_sources.push(target.src_path.clone());
            }
        }
    }
    inputs
}

fn canonical_workspace_path(
    path: &Path,
    kind: &str,
    canonical_root: &Path,
    violations: &mut Vec<String>,
) -> Option<PathBuf> {
    let canonical_path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) => {
            violations.push(format!(
                "{kind} {} cannot be resolved: {error}",
                path.display()
            ));
            return None;
        },
    };
    if !canonical_path.starts_with(canonical_root) {
        violations.push(format!(
            "{kind} {} resolves outside canonical project root {}",
            path.display(),
            canonical_root.display()
        ));
        return None;
    }
    Some(canonical_path)
}

fn cargo_target_source_paths(
    manifest_path: &Path,
    canonical_root: &Path,
    violations: &mut Vec<String>,
) -> io::Result<Vec<PathBuf>> {
    let canonical_manifest = match fs::canonicalize(manifest_path) {
        Ok(path) => path,
        Err(error) => {
            violations.push(format!(
                "{} cannot be resolved: {error}",
                manifest_path.display()
            ));
            return Ok(Vec::new());
        },
    };
    if !canonical_manifest.starts_with(canonical_root) {
        violations.push(format!(
            "{} resolves outside canonical project root {}",
            manifest_path.display(),
            canonical_root.display()
        ));
        return Ok(Vec::new());
    }

    let source = fs::read_to_string(&canonical_manifest)?;
    let manifest = match toml::from_str::<toml::Value>(&source) {
        Ok(manifest) => manifest,
        Err(error) => {
            violations.push(format!(
                "{} is not valid TOML: {error}",
                canonical_manifest.display()
            ));
            return Ok(Vec::new());
        },
    };
    let Some(root_table) = manifest.as_table() else {
        violations.push(format!(
            "{} does not contain a TOML table",
            canonical_manifest.display()
        ));
        return Ok(Vec::new());
    };
    let Some(manifest_directory) = canonical_manifest.parent() else {
        violations.push(format!(
            "{} has no parent directory",
            canonical_manifest.display()
        ));
        return Ok(Vec::new());
    };

    let mut paths = Vec::new();
    if let Some(package) = root_table.get("package") {
        add_cargo_build_script_path(
            package,
            &canonical_manifest,
            manifest_directory,
            &mut paths,
            violations,
        );
    }
    if let Some(target) = root_table.get("lib") {
        add_cargo_target_path(
            target,
            "lib",
            &canonical_manifest,
            manifest_directory,
            &mut paths,
            violations,
        );
    }
    for target_kind in ["bin", "test", "example", "bench"] {
        let Some(targets) = root_table.get(target_kind) else {
            continue;
        };
        let Some(targets) = targets.as_array() else {
            violations.push(format!(
                "{} has non-array [[{target_kind}]] targets",
                canonical_manifest.display()
            ));
            continue;
        };
        for target in targets {
            add_cargo_target_path(
                target,
                target_kind,
                &canonical_manifest,
                manifest_directory,
                &mut paths,
                violations,
            );
        }
    }
    Ok(paths)
}

fn add_cargo_build_script_path(
    package: &toml::Value,
    manifest_path: &Path,
    manifest_directory: &Path,
    paths: &mut Vec<PathBuf>,
    violations: &mut Vec<String>,
) {
    let Some(package) = package.as_table() else {
        violations.push(format!(
            "{} has a non-table package definition",
            manifest_path.display()
        ));
        return;
    };
    let Some(build_script) = package.get("build") else {
        return;
    };
    if build_script.as_bool().is_some_and(|enabled| !enabled) {
        return;
    }
    let Some(path) = build_script.as_str() else {
        violations.push(format!(
            "{} has a malformed package.build; custom Cargo build-script paths must be literal strings or false",
            manifest_path.display()
        ));
        return;
    };
    paths.push(manifest_directory.join(path));
}

fn add_cargo_target_path(
    target: &toml::Value,
    target_kind: &str,
    manifest_path: &Path,
    manifest_directory: &Path,
    paths: &mut Vec<PathBuf>,
    violations: &mut Vec<String>,
) {
    let Some(target) = target.as_table() else {
        violations.push(format!(
            "{} has a non-table {target_kind} target",
            manifest_path.display()
        ));
        return;
    };
    let Some(path) = target.get("path") else {
        return;
    };
    let Some(path) = path.as_str() else {
        violations.push(format!(
            "{} has a non-string {target_kind}.path; custom Cargo target paths must be literal strings",
            manifest_path.display()
        ));
        return;
    };
    paths.push(manifest_directory.join(path));
}

fn scan_source_file(
    source_path: &Path,
    canonical_root: &Path,
    visited: &mut BTreeSet<(PathBuf, PathBuf)>,
    violations: &mut Vec<String>,
) -> io::Result<()> {
    let canonical_path = match fs::canonicalize(source_path) {
        Ok(path) => path,
        Err(error) => {
            violations.push(format!(
                "{} cannot be resolved: {error}",
                source_path.display()
            ));
            return Ok(());
        },
    };
    if !canonical_path.starts_with(canonical_root) {
        violations.push(format!(
            "{} resolves outside canonical project root {}",
            source_path.display(),
            canonical_root.display()
        ));
        return Ok(());
    }
    let Some(parent) = source_path.parent() else {
        violations.push(format!("{} has no source directory", source_path.display()));
        return Ok(());
    };
    let include_directory = fs::canonicalize(parent)?;
    // Rust resolves include! beside the source path used by the compiler. Two
    // symlinks to one file can therefore consume different included files.
    if !visited.insert((canonical_path.clone(), include_directory.clone())) {
        return Ok(());
    }

    let source = fs::read_to_string(&canonical_path)?;
    let tokens = match TokenStream::from_str(&source) {
        Ok(tokens) => tokens,
        Err(error) => {
            violations.push(format!(
                "{} is not tokenizable Rust source: {error}",
                canonical_path.display()
            ));
            return Ok(());
        },
    };

    let count = allow_violations_in_tokens(&tokens);
    if count != 0 {
        violations.push(format!(
            "{} has #[allow] attributes, broad #[expect] attributes, or attributes forwarded from macro metavariables ({count}); use one lint in a reasoned #[expect] on the smallest enclosing statement or item (AGENTS.md)",
            canonical_path.display()
        ));
    }

    for reference in source_references(&tokens) {
        match reference {
            SourceReference::LiteralInclude(relative_path) => {
                scan_source_file(
                    &include_directory.join(relative_path),
                    canonical_root,
                    visited,
                    violations,
                )?;
            }
            SourceReference::OpaqueInclude => violations.push(format!(
                "{} uses a non-literal include!; use a project-relative string literal so the policy scanner can inspect the compiler input",
                canonical_path.display()
            )),
            SourceReference::CustomModulePath => violations.push(format!(
                "{} uses #[path]; use conventional mod file layout so the policy scanner can enumerate every compiler input",
                canonical_path.display()
            )),
        }
    }
    Ok(())
}

#[test]
fn accepts_fixed_documentation_attributes_with_macro_values() {
    let source = r#"
        macro_rules! documented {
            ($description:literal) => {
                #[doc = $description]
                pub struct Item;
            };
        }
        documented!("A documented item.");
        "#;
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        0
    );
}

#[test]
fn requires_lint_inheritance_for_root_and_nested_workspace_packages() {
    let root = policy_fixture_root("workspace-lints").expect("fixture root should be creatable");
    fs::create_dir_all(root.join("src")).expect("root sources should be creatable");
    fs::create_dir_all(root.join("member/src")).expect("member sources should be creatable");
    let root_manifest = r#"
        [package]
        name = "root-package"
        version = "0.1.0"
        edition = "2024"
        [workspace]
        members = ["member"]
        [workspace.lints.rust]
        unsafe_code = "forbid"
        "#;
    let member_manifest = r#"
        [package]
        name = "member-package"
        version = "0.1.0"
        edition = "2024"
        "#;
    fs::write(root.join("src/lib.rs"), "").expect("root source should be writable");
    fs::write(root.join("member/src/lib.rs"), "").expect("member source should be writable");
    fs::write(root.join("Cargo.toml"), root_manifest).expect("root manifest should be writable");
    fs::write(root.join("member/Cargo.toml"), member_manifest)
        .expect("member manifest should be writable");
    write_fixture_lock(&root, &["member-package", "root-package"])
        .expect("lock should be writable");

    let violations = project_policy_violations(&root).expect("workspace should be readable");
    assert_eq!(violations.len(), 2, "{violations:?}");
    assert!(
        violations
            .iter()
            .all(|violation| violation.contains("[lints] workspace = true"))
    );

    for (manifest, source) in [
        ("Cargo.toml", root_manifest),
        ("member/Cargo.toml", member_manifest),
    ] {
        fs::write(
            root.join(manifest),
            format!("{source}\n[lints]\nworkspace = true\n"),
        )
        .expect("lint inheritance should be writable");
    }
    let violations = project_policy_violations(&root).expect("workspace should be readable");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn rejects_crate_inner_allow_with_a_reason() {
    let source = "#![allow(clippy::unwrap_used, reason = \"claimed exception\")]\nfn main() {}";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        1
    );
}

#[test]
fn rejects_expectations_naming_several_lints() {
    let source = r#"
        #[expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "claimed exception")]
        fn direct() {}

        #[cfg_attr(test, expect(dead_code, unused_variables, reason = "claimed exception"))]
        fn conditional() {}
        "#;
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        2
    );
}

#[test]
fn rejects_expectations_naming_lint_groups() {
    let source = r#"
        #![expect(warnings, reason = "claimed exception")]

        #[expect(clippy::pedantic, reason = "claimed exception")]
        fn clippy_group() {}

        #[cfg_attr(test, expect(rust_2018_idioms, reason = "claimed exception"))]
        fn rustc_group() {}

        #[expect(rustdoc::all, reason = "claimed exception")]
        fn rustdoc_group() {}
        "#;
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        4
    );
}

#[test]
fn rejects_expectations_on_the_allow_attribute_lints() {
    let source = r#"
        #[expect(clippy::allow_attributes, reason = "claimed exception")]
        fn bare_allow() {}

        #[cfg_attr(test, expect(clippy::allow_attributes_without_reason, reason = "claimed exception"))]
        fn unexplained_allow() {}
        "#;
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        2
    );
}

#[test]
fn accepts_single_lint_reasoned_expectations() {
    let source = r#"
        #[expect(clippy::unwrap_used, reason = "the parser accepted this input")]
        fn direct() {}

        #[cfg_attr(test, expect(dead_code, reason = "only tests call this"))]
        fn conditional() {}

        #[expect(rust_2024_prelude_collisions, reason = "an edition lint, not its group")]
        fn edition_lint() {}

        mod module {
            #[expect(clippy::module_inception, reason = "Clippy reports the module itself")]
            mod module {}
        }
        "#;
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        0
    );
}

#[test]
fn lint_groups_match_the_toolchain() {
    let mut toolchain_groups = BTreeSet::new();
    for tool in ["clippy-driver", "rustdoc"] {
        let output = Command::new(tool)
            .args(["-W", "help"])
            .current_dir(project_root())
            .output()
            .unwrap_or_else(|error| panic!("cannot run {tool}: {error}"));
        assert!(
            output.status.success(),
            "{tool} -W help failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        toolchain_groups.extend(lint_group_names(&String::from_utf8_lossy(&output.stdout)));
    }
    let listed: BTreeSet<String> = LINT_GROUPS.map(str::to_owned).into();
    assert_eq!(
        listed, toolchain_groups,
        "LINT_GROUPS must list every lint group of the pinned toolchain"
    );
}

/// Reads the name column of each `Lint groups` table in `-W help` output.
fn lint_group_names(help: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_groups = false;
    let mut in_table = false;
    for line in help.lines().map(str::trim) {
        if line.starts_with("Lint groups") {
            in_groups = true;
            in_table = false;
        } else if in_groups && line.starts_with("----") {
            in_table = true;
        } else if in_table {
            if let Some(name) = line.split_whitespace().next() {
                names.push(name.replace('-', "_"));
            } else {
                in_groups = false;
                in_table = false;
            }
        }
    }
    names
}

#[test]
fn rejects_crate_level_expectations_outside_test_targets() {
    let crate_roots = [
        "src/lib.rs",
        "src/main.rs",
        "src/bin/tool.rs",
        "build.rs",
        "examples/demo.rs",
        "benches/speed.rs",
    ];
    let violations = fixture_violations(
        "crate-level-expectations",
        &crate_roots.map(|crate_root| {
            (
                crate_root,
                "#![cfg_attr(unix, expect(clippy::unwrap_used, reason = \"claimed crate-wide exception\"))]\nfn main() {}",
            )
        }),
    )
    .expect("fixture project should be readable");

    assert_eq!(violations.len(), crate_roots.len(), "{violations:?}");
    for crate_root in crate_roots {
        assert!(
            violations.iter().any(|violation| {
                violation.contains(crate_root) && violation.contains("crate-level #![expect]")
            }),
            "crate-level expectation in {crate_root} was not rejected: {violations:?}"
        );
    }
}

#[test]
fn accepts_crate_level_expectations_in_test_targets() {
    let violations = fixture_violations(
        "test-target-expectations",
        &[
            ("src/lib.rs", ""),
            (
                "tests/integration.rs",
                "#![expect(clippy::unwrap_used, reason = \"tests unwrap values they constructed\")]\n",
            ),
        ],
    )
    .expect("fixture project should be readable");

    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn rejects_crate_level_arithmetic_expectations_in_test_targets() {
    let violations = fixture_violations(
        "test-target-arithmetic",
        &[
            ("src/lib.rs", ""),
            (
                "tests/properties.rs",
                "#![expect(\n    clippy::arithmetic_side_effects,\n    reason = \"value is drawn from 0..=u32::MAX / 2, so value * 2 cannot overflow\"\n)]\n",
            ),
            (
                "tests/conditional.rs",
                "#![cfg_attr(unix, expect(clippy::arithmetic_side_effects, reason = \"bounded\"))]\n",
            ),
        ],
    )
    .expect("fixture project should be readable");

    assert_eq!(violations.len(), 2, "{violations:?}");
    for crate_root in ["tests/properties.rs", "tests/conditional.rs"] {
        assert!(
            violations.iter().any(|violation| {
                violation.contains(crate_root) && violation.contains("arithmetic expectation")
            }),
            "arithmetic expectation in {crate_root} was not rejected: {violations:?}"
        );
    }
}

#[test]
fn rejects_module_wide_panic_expectations_outside_test_code() {
    let violations = fixture_violations(
        "module-expectations",
        &[
            (
                "src/lib.rs",
                r#"
                mod imp;
                #[expect(clippy::unwrap_used, reason = "claimed module-wide exception")]
                mod outer {}
                mod inline {
                    #![cfg_attr(unix, expect(clippy::indexing_slicing, reason = "claimed module-wide exception"))]
                }
                macro_rules! module {
                    ($name:ident) => {
                        #[expect(clippy::unwrap_used, reason = "claimed module-wide exception")]
                        mod $name {}
                    };
                }
                #[cfg(test)]
                mod tests {
                    #![expect(clippy::unwrap_used, reason = "tests unwrap values they constructed")]
                }
                #[cfg(test)]
                mod arithmetic_tests {
                    #![expect(clippy::arithmetic_side_effects, reason = "claimed module-wide exception")]
                }
                #[expect(clippy::module_inception, reason = "the inner module mirrors its parent")]
                mod checks {
                    mod checks {}
                }
                #[expect(clippy::unwrap_used, reason = "the caller validated the text")]
                pub fn parse(text: &str) -> u8 {
                    text.parse().unwrap()
                }
                "#,
            ),
            (
                "src/imp.rs",
                "#![expect(clippy::unwrap_used, reason = \"claimed module-wide exception\")]\n",
            ),
            (
                "tests/integration.rs",
                "mod common;\n#[expect(clippy::arithmetic_side_effects, reason = \"claimed module-wide exception\")]\nmod more {}\n",
            ),
            (
                "tests/common/mod.rs",
                "#![expect(clippy::unwrap_used, reason = \"helpers unwrap fixtures they wrote\")]\n",
            ),
        ],
    )
    .expect("fixture project should be readable");

    assert_eq!(violations.len(), 3, "{violations:?}");
    for (file, count) in [
        ("src/lib.rs", 4),
        ("src/imp.rs", 1),
        ("tests/integration.rs", 1),
    ] {
        assert!(
            violations.iter().any(|violation| {
                violation.contains(file)
                    && violation.contains(&format!("expectations of panic lints ({count})"))
            }),
            "module-wide expectations in {file} were not counted: {violations:?}"
        );
    }
}

#[test]
fn rejects_impl_and_trait_wide_panic_expectations_outside_test_code() {
    let violations = fixture_violations(
        "block-expectations",
        &[(
            "src/lib.rs",
            r#"
            pub struct Parser;
            #[expect(clippy::unwrap_used, reason = "claimed impl-wide exception")]
            impl Parser {}
            impl Parser {
                #![expect(clippy::indexing_slicing, reason = "claimed impl-wide exception")]
                pub fn first(values: &[u8]) -> u8 {
                    values[0]
                }
            }
            pub trait Parse {
                #![expect(clippy::unwrap_used, reason = "claimed trait-wide exception")]
                fn parse(text: &str) -> u8 {
                    text.parse().unwrap()
                }
            }
            #[expect(clippy::panic, reason = "claimed impl-wide exception")]
            unsafe impl Sync for Parser {}
            impl Parser {
                #[expect(clippy::unwrap_used, reason = "the caller validated the text")]
                pub fn parse(text: &str) -> u8 {
                    text.parse().unwrap()
                }
            }
            pub fn values() -> impl Iterator<Item = u8> {
                #![expect(clippy::unwrap_used, reason = "the literal parses")]
                Some("1".parse::<u8>().unwrap()).into_iter()
            }
            #[cfg(test)]
            impl Parser {
                #![expect(clippy::unwrap_used, reason = "tests unwrap values they constructed")]
            }
            "#,
        )],
    )
    .expect("fixture project should be readable");

    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("src/lib.rs")
            && violations[0].contains("expectations of panic lints (4)"),
        "impl- and trait-wide expectations were not counted: {violations:?}"
    );
}

#[test]
fn rejects_assertions_outside_test_code() {
    let violations = fixture_violations(
        "assertions",
        &[
            (
                "src/lib.rs",
                r#"
                mod checks;
                mod nested;
                mod directory;
                mod inner_tests;
                #[cfg(test)]
                mod tests;
                mod inline {
                    pub(crate) fn check(value: u8) {
                        assert_eq!(value, 1);
                    }
                    mod deeper;
                }
                include!("body.rs");
                use std::assert as check;
                macro_rules! require {
                    ($condition:expr) => {
                        std::assert!($condition)
                    };
                }
                macro_rules! call {
                    ($assert:ident) => {
                        $assert!(true)
                    };
                }
                static LAZY: std::sync::LazyLock<u8> = std::sync::LazyLock::new(|| {
                    assert_ne!(1, 2);
                    1
                });
                static TABLE: [u8; 2] = {
                    assert!(u8::MAX == 255);
                    [1, 2]
                };
                const _: () = assert!(u8::MAX == 255);
                const _: () = if usize::BITS >= 32 { assert!(true) } else { assert!(true) };
                const _: () = {
                    const fn nonzero(value: u8) -> bool {
                        value != 0
                    }
                    assert!(nonzero(1));
                };
                const _: () = {
                    macro_rules! expand {
                        ($condition:expr) => {
                            assert!($condition)
                        };
                    }
                };
                pub const HOOK: fn(u8) = {
                    fn hook(value: u8) {
                        assert!(value > 0);
                    }
                    hook
                };
                const _: () = include!("shared.rs");
                pub fn shared() {
                    include!("shared.rs")
                }
                pub const fn checked() -> u8 {
                    const { assert!(usize::BITS >= 32) };
                    debug_assert!(true);
                    1
                }
                pub fn pair() -> (u8, u8) {
                    (1, const { assert!(usize::BITS >= 32); 2 })
                }
                pub fn compare(assert: u8) -> bool {
                    assert != 0
                }
                pub const unsafe fn first() {}
                pub fn items<const N: usize>() -> impl Iterator<Item = u8> {
                    assert!(N > 0);
                    [].into_iter()
                }
                pub fn classify(event: Event) -> u8 {
                    match event {
                        #[cfg(test)]
                        Event::Probe => 0,
                        Event::Value(value) => {
                            assert!(value > 0);
                            value
                        }
                    }
                }
                #[cfg(test)]
                impl<A, B> Pair<A, B> {
                    fn check(&self) {
                        assert!(true);
                    }
                }
                #[cfg(all(test, unix))]
                fn unix_test_helper() {
                    assert!(true);
                }
                #[cfg(any(test, feature = "testing"))]
                fn shared_helper() {
                    assert!(true);
                }
                "#,
            ),
            (
                "src/checks.rs",
                r"
                pub(crate) fn check(value: u8) {
                    assert!(value > 0);
                }
                const HOOK: fn(u8) = move |value| assert!(value > 0);
                #[test]
                fn checks_value() {
                    assert!(true);
                }
                #[tokio::test]
                async fn checks_asynchronously() {
                    assert!(true);
                }
                ",
            ),
            ("src/nested.rs", "mod sub;\n"),
            (
                "src/nested/sub.rs",
                "pub fn sub() {\n    assert!(true);\n}\n",
            ),
            (
                "src/directory/mod.rs",
                "pub fn directory() {\n    assert!(true);\n}\n",
            ),
            (
                "src/inline/deeper.rs",
                "pub fn deeper() {\n    assert!(true);\n}\n",
            ),
            (
                "src/body.rs",
                "pub fn included() {\n    assert!(true);\n}\n",
            ),
            ("src/shared.rs", "assert!(true)\n"),
            (
                "src/inner_tests.rs",
                "#![cfg(test)]\nfn helper() {\n    assert!(true);\n}\n",
            ),
            ("src/tests.rs", "fn helper() {\n    assert!(true);\n}\n"),
            ("tests/integration.rs", "mod common;\n"),
            (
                "tests/common/mod.rs",
                "pub fn helper() {\n    assert!(true);\n}\n",
            ),
        ],
    )
    .expect("fixture project should be readable");

    let expected = [
        ("src/lib.rs", 9),
        ("src/checks.rs", 2),
        ("src/nested/sub.rs", 1),
        ("src/directory/mod.rs", 1),
        ("src/inline/deeper.rs", 1),
        ("src/body.rs", 1),
        ("src/shared.rs", 1),
    ];
    assert_eq!(violations.len(), expected.len(), "{violations:?}");
    for (file, count) in expected {
        assert!(
            violations.iter().any(|violation| {
                violation.contains(file)
                    && violation.contains(&format!("outside test code ({count})"))
            }),
            "assertions in {file} were not counted: {violations:?}"
        );
    }
}

#[test]
fn rejects_module_declarations_the_walk_cannot_resolve() {
    let violations = fixture_violations(
        "unresolved-modules",
        &[
            (
                "src/lib.rs",
                r#"
                mod missing;
                macro_rules! declare {
                    ($name:ident) => {
                        mod $name;
                    };
                }
                #[path = "elsewhere.rs"]
                mod custom;
                #[cfg(windows)]
                mod platform;
                "#,
            ),
            ("src/elsewhere.rs", ""),
            ("src/platform.rs", ""),
        ],
    )
    .expect("fixture project should be readable");

    assert_eq!(violations.len(), 2, "{violations:?}");
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("cannot find (2)")),
        "unresolved modules were not counted: {violations:?}"
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("uses #[path]")),
        "the custom module path was not rejected: {violations:?}"
    );
}

#[test]
fn denied_clippy_lints_are_classified_by_panic_scope() {
    let source =
        fs::read_to_string(project_root().join("Cargo.toml")).expect("manifest should be readable");
    let manifest: toml::Value = toml::from_str(&source).expect("manifest should parse");
    let clippy = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .and_then(|lints| lints.get("clippy"))
        .and_then(toml::Value::as_table)
        .expect("[workspace.lints.clippy] should be a table");
    let denied: BTreeSet<String> = clippy
        .iter()
        .filter(|(_, value)| {
            matches!(
                value
                    .as_str()
                    .or_else(|| value.get("level").and_then(toml::Value::as_str)),
                Some("deny" | "forbid")
            )
        })
        .map(|(name, _)| format!("clippy::{name}"))
        .filter(|lint| !LINT_GROUPS.contains(&lint.as_str()))
        .collect();
    let classified: BTreeSet<String> = PANIC_LINTS
        .iter()
        .chain(&NON_PANIC_LINTS)
        .chain(&ALLOW_ATTRIBUTE_LINTS)
        .map(|lint| (*lint).to_owned())
        .collect();
    assert_eq!(
        denied, classified,
        "each lint [workspace.lints.clippy] denies must sit in PANIC_LINTS, NON_PANIC_LINTS, or ALLOW_ATTRIBUTE_LINTS, and each listed lint must be denied"
    );
}

#[test]
fn rejects_policy_attributes_behind_boolean_cfg_attr_predicates() {
    let source = r#"
        #[cfg_attr(true, allow(dead_code))]
        fn allowed() {}

        #[cfg_attr(true, expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "claimed exception"))]
        fn several() {}

        #[cfg_attr(true, cfg_attr(false, expect(clippy::pedantic, reason = "claimed exception")))]
        fn group() {}
        "#;
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        3
    );
    let crate_root =
        "#![cfg_attr(true, expect(clippy::unwrap_used, reason = \"claimed exception\"))]";
    assert_eq!(
        crate_level_expectation_count(
            &TokenStream::from_str(crate_root).expect("valid Rust tokens"),
            is_expectation
        ),
        1
    );
    let module = "#[cfg_attr(true, path = \"payload.inc\")] mod payload;";
    assert_eq!(
        source_references(&TokenStream::from_str(module).expect("valid Rust tokens")).len(),
        1
    );
}

#[test]
fn rejects_raw_identifier_allow() {
    let source = "#![r#allow(dead_code)]\nfn main() {}";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        1
    );
}

#[test]
fn rejects_multiline_and_cfg_attr_allows() {
    let source = r"
        #[
            allow(dead_code)
        ]
        fn direct() {}

        #[cfg_attr(unix, cfg_attr(test, allow(clippy::unwrap_used)))]
        fn conditional() {}
        ";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        2
    );
}

#[test]
fn rejects_raw_identifier_cfg_attr_with_nested_allow() {
    let source = "#[r#cfg_attr(unix, cfg_attr(test, r#allow(dead_code)))]\nfn conditional() {}";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        1
    );
}

#[test]
fn rejects_allow_generated_inside_a_macro_body() {
    let source = "macro_rules! generated { () => { #[allow(dead_code)] fn hidden() {} }; }";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        1
    );
}

#[test]
fn rejects_macro_attribute_forwarding_metavariables() {
    let source = r"
        macro_rules! meta_attr { ($attribute:meta) => { #[$attribute] struct Meta; }; }
        macro_rules! ident_attr { ($attribute:ident) => { #[$attribute] struct Ident; }; }
        macro_rules! tt_attr { ($attribute:tt) => { #[$attribute] struct TokenTree; }; }
        ";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        3
    );
}

#[test]
fn rejects_semicolon_wrapped_allow_forwarding() {
    let source = r"
        macro_rules! with_attr { ($attribute:meta;) => { #[$attribute] struct Generated; }; }
        with_attr!(allow(dead_code););
        ";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        1
    );
}

#[test]
fn rejects_repeated_and_cfg_attr_metavariable_emission() {
    let source = r"
        macro_rules! repeated { ($($attribute:meta),*) => { $(#[$attribute] struct Generated;)* }; }
        macro_rules! conditional { ($attribute:meta) => { #[cfg_attr(test, $attribute)] struct Conditional; }; }
        ";
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        2
    );
}

#[test]
fn ignores_attribute_text_in_comments_and_literals() {
    let source = r####"
        // #![allow(dead_code)]
        const TEXT: &str = r###"#[allow(dead_code)]"###;
        #[cfg_attr(feature = "allow", derive(Clone))]
        #[expect(dead_code, reason = "the probe is intentionally unused")]
        struct Probe;
        "####;
    assert_eq!(
        allow_policy_violation_count(source).expect("valid Rust tokens"),
        0
    );
}

#[test]
fn follows_literal_includes_cycle_safely() {
    let root = policy_fixture_root("literal-include").expect("fixture root should be creatable");
    fs::write(root.join("lib.rs"), "include!(\"payload.inc\");")
        .expect("fixture root source should be writable");
    fs::write(
        root.join("payload.inc"),
        "#[allow(dead_code)] fn included() {}\ninclude!(\"lib.rs\");",
    )
    .expect("included fixture source should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("payload.inc"));
}

#[test]
fn follows_all_cargo_custom_target_source_paths() {
    let root =
        policy_fixture_root("cargo-custom-targets").expect("fixture root should be creatable");
    fs::create_dir_all(root.join("src")).expect("fixture source directory should be creatable");
    fs::write(
        root.join("Cargo.toml"),
        r#"
            [package]
            name = "policy-fixture"
            version = "0.1.0"
            edition = "2024"
            build = "src/build-script.inc"

            [lib]
            path = "src/library.inc"

            [[bin]]
            name = "command"
            path = "src/command.inc"

            [[test]]
            name = "integration"
            path = "src/integration.inc"

            [[example]]
            name = "example"
            path = "src/example.inc"

            [[bench]]
            name = "benchmark"
            path = "src/benchmark.inc"

            [workspace]
            [workspace.lints]
            [lints]
            workspace = true
        "#,
    )
    .expect("fixture manifest should be writable");
    for source in [
        "library.inc",
        "command.inc",
        "integration.inc",
        "example.inc",
        "benchmark.inc",
        "build-script.inc",
    ] {
        fs::write(
            root.join("src").join(source),
            "#[allow(dead_code)] fn target() {}",
        )
        .expect("custom target source should be writable");
    }
    write_fixture_lock(&root, &["policy-fixture"]).expect("fixture lock should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 6);
    for source in [
        "library.inc",
        "command.inc",
        "integration.inc",
        "example.inc",
        "benchmark.inc",
        "build-script.inc",
    ] {
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(source)),
            "custom target {source} was not scanned: {violations:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn follows_includes_relative_to_each_symlinked_cargo_target() {
    use std::os::unix::fs::symlink;

    let root =
        policy_fixture_root("symlink-include-context").expect("fixture root should be creatable");
    for directory in ["first", "second", "shared"] {
        fs::create_dir_all(root.join(directory)).expect("source directory should be creatable");
    }
    fs::write(
        root.join("Cargo.toml"),
        r#"
            [package]
            name = "symlink-include-context"
            version = "0.1.0"
            edition = "2024"

            [[bin]]
            name = "first"
            path = "first/main.inc"

            [[bin]]
            name = "second"
            path = "second/main.inc"

            [workspace]
            [workspace.lints]
            [lints]
            workspace = true
        "#,
    )
    .expect("fixture manifest should be writable");
    write_fixture_lock(&root, &["symlink-include-context"])
        .expect("fixture lock should be writable");
    fs::write(root.join("shared/main.inc"), "include!(\"payload.inc\");")
        .expect("shared source should be writable");
    fs::write(
        root.join("shared/payload.inc"),
        "compile_error!(\"include must use the source symlink directory\");",
    )
    .expect("unused canonical sibling should be writable");
    for directory in ["first", "second"] {
        symlink("../shared/main.inc", root.join(directory).join("main.inc"))
            .expect("source symlink should be creatable");
    }
    fs::write(root.join("first/payload.inc"), "fn main() {}")
        .expect("first included source should be writable");
    fs::write(
        root.join("second/payload.inc"),
        "#[allow(dead_code)] fn main() {}",
    )
    .expect("second included source should be writable");

    let output = Command::new(env!("CARGO"))
        .args(["check", "--locked", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(root.join("target"))
        .output()
        .expect("Cargo should validate the fixture's real include resolution");
    assert!(
        output.status.success(),
        "the supported symlink layout must compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let violations = project_policy_violations(&root).expect("fixture should be readable");
    assert_eq!(violations.len(), 1, "unexpected violations: {violations:?}");
    assert!(violations[0].contains("second/payload.inc"));

    fs::write(root.join("second/payload.inc"), "fn main() {}")
        .expect("clean included source should be writable");
    let violations = project_policy_violations(&root).expect("fixture should be readable");
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn rejects_opaque_and_out_of_root_cargo_target_paths() {
    let root =
        policy_fixture_root("invalid-cargo-targets").expect("fixture root should be creatable");
    let external = root
        .parent()
        .expect("fixture root should have a parent")
        .join(format!("external-target-{}.inc", std::process::id()));
    fs::write(&external, "fn external() {}").expect("external target source should be writable");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"
                [package]
                name = "policy-fixture"
                version = "0.1.0"
                edition = "2024"
                build = true

                [lib]
                path = {{ workspace = true }}

                [[bin]]
                name = "external"
                path = "../{}"

                [workspace]
            "#,
            external
                .file_name()
                .and_then(|name| name.to_str())
                .expect("external fixture name should be UTF-8")
        ),
    )
    .expect("fixture manifest should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("locked Cargo metadata failed"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("package.build"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("literal strings"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("outside canonical project root"))
    );
}

#[test]
fn accepts_an_explicitly_disabled_cargo_build_script() {
    let root =
        policy_fixture_root("disabled-build-script").expect("fixture root should be creatable");
    fs::create_dir_all(root.join("src")).expect("fixture source directory should be creatable");
    fs::write(
        root.join("Cargo.toml"),
        r#"
            [package]
            name = "policy-fixture"
            version = "0.1.0"
            edition = "2024"
            build = false

            [workspace]
            [workspace.lints]
            [lints]
            workspace = true
        "#,
    )
    .expect("fixture manifest should be writable");
    fs::write(root.join("src/lib.rs"), "fn ordinary() {}")
        .expect("fixture source should be writable");
    write_fixture_lock(&root, &["policy-fixture"]).expect("fixture lock should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn rejects_workspace_members_outside_the_canonical_project_root() {
    let container = std::env::temp_dir().join(format!(
        "rust-policy-external-workspace-{}",
        std::process::id()
    ));
    if container.exists() {
        fs::remove_dir_all(&container).expect("stale fixture should be removable");
    }
    let root = container.join("project");
    let external = container.join("member");
    fs::create_dir_all(&root).expect("fixture root should be creatable");
    fs::create_dir_all(&external).expect("external member directory should be creatable");
    fs::write(
        root.join("Cargo.toml"),
        r#"
            [workspace]
            resolver = "3"
            members = ["../member"]
        "#,
    )
    .expect("workspace manifest should be writable");
    fs::write(
        external.join("Cargo.toml"),
        r#"
            [package]
            name = "external-workspace-member"
            version = "0.1.0"
            edition = "2024"
            workspace = "../project"

            [lib]
            path = "lib.rs"
        "#,
    )
    .expect("external member manifest should be writable");
    fs::write(external.join("lib.rs"), "fn external() {}")
        .expect("external member source should be writable");
    write_fixture_lock(&root, &["external-workspace-member"])
        .expect("workspace lock should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert!(
        violations.iter().any(|violation| {
            violation.contains("workspace member manifest")
                && violation.contains("outside canonical project root")
        }),
        "external workspace member was not rejected: {violations:?}"
    );
    fs::remove_dir_all(container).expect("fixture should be removable");
}

#[test]
fn scans_internal_workspace_targets_without_scanning_external_path_dependencies() {
    let root =
        policy_fixture_root("workspace-member-scope").expect("fixture root should be creatable");
    let member = root.join("vendor/member");
    fs::create_dir_all(&member).expect("internal member directory should be creatable");
    let dependency_name = format!("external-path-dependency-{}", std::process::id());
    let dependency = root
        .parent()
        .expect("fixture root should have a parent")
        .join(&dependency_name);
    fs::create_dir_all(&dependency).expect("external dependency directory should be creatable");

    fs::write(
        root.join("Cargo.toml"),
        r#"
            [workspace]
            resolver = "3"
            members = ["vendor/member"]
            [workspace.lints]
        "#,
    )
    .expect("workspace manifest should be writable");
    fs::write(
        member.join("Cargo.toml"),
        format!(
            r#"
                [package]
                name = "internal-workspace-member"
                version = "0.1.0"
                edition = "2024"

                [dependencies]
                external-path-dependency = {{ path = "../../../{dependency_name}" }}

                [lib]
                path = "compiler-input"

                [lints]
                workspace = true
            "#,
        ),
    )
    .expect("internal member manifest should be writable");
    fs::write(member.join("compiler-input"), "include!(\"payload.inc\");")
        .expect("internal target should be writable");
    fs::write(
        member.join("payload.inc"),
        "#[allow(dead_code)] fn internal_member() {}",
    )
    .expect("internal included source should be writable");
    fs::write(
        dependency.join("Cargo.toml"),
        r#"
            [package]
            name = "external-path-dependency"
            version = "0.1.0"
            edition = "2024"

            [lib]
            path = "lib.rs"
        "#,
    )
    .expect("external dependency manifest should be writable");
    fs::write(
        dependency.join("lib.rs"),
        "#[allow(dead_code)] fn external_dependency() {}",
    )
    .expect("external dependency source should be writable");
    fs::write(
        root.join("Cargo.lock"),
        r#"# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = "external-path-dependency"
version = "0.1.0"

[[package]]
name = "internal-workspace-member"
version = "0.1.0"
dependencies = [
 "external-path-dependency",
]
"#,
    )
    .expect("workspace lock should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 1, "unexpected violations: {violations:?}");
    assert!(violations[0].contains("payload.inc"));
    assert!(!violations[0].contains("external-path-dependency"));
}

#[test]
fn scans_cfg_disabled_conventional_modules_in_root_pruned_workspace_members() {
    let root = policy_fixture_root("workspace-member-conventional-module")
        .expect("fixture root should be creatable");
    let member = root.join("vendor/member");
    fs::create_dir_all(member.join("src")).expect("member source directory should be creatable");
    fs::create_dir_all(member.join("target")).expect("member target directory should be creatable");
    fs::create_dir_all(member.join("vendor")).expect("member vendor directory should be creatable");
    fs::write(
        root.join("Cargo.toml"),
        r#"
            [workspace]
            resolver = "3"
            members = ["vendor/member"]
            [workspace.lints]
        "#,
    )
    .expect("workspace manifest should be writable");
    fs::write(
        member.join("Cargo.toml"),
        r#"
            [package]
            name = "conventional-module-member"
            version = "0.1.0"
            edition = "2024"

            [lints]
            workspace = true
        "#,
    )
    .expect("member manifest should be writable");
    fs::write(member.join("src/lib.rs"), "#[cfg(windows)] mod hidden;")
        .expect("member crate root should be writable");
    fs::write(
        member.join("src/hidden.rs"),
        "#![allow(dead_code)]\npub fn hidden() {}",
    )
    .expect("cfg-disabled conventional module should be writable");
    for generated in [
        member.join("target/generated.rs"),
        member.join("vendor/dependency.rs"),
    ] {
        fs::write(generated, "#![allow(dead_code)]\nfn generated() {}")
            .expect("generated source should be writable");
    }
    write_fixture_lock(&root, &["conventional-module-member"])
        .expect("workspace lock should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 1, "unexpected violations: {violations:?}");
    assert!(violations[0].contains("src/hidden.rs"));
}

#[test]
fn rejects_opaque_includes_and_custom_module_paths() {
    let root = policy_fixture_root("opaque-inputs").expect("fixture root should be creatable");
    fs::write(
        root.join("lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\n#[path = \"payload.inc\"] mod payload;\n#[cfg_attr(test, path = \"conditional.inc\")] mod conditional;",
    )
    .expect("fixture root source should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 3);
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("non-literal include!"))
    );
    assert_eq!(
        violations
            .iter()
            .filter(|violation| violation.contains("uses #[path]"))
            .count(),
        2
    );
}

#[test]
fn prunes_generated_roots_without_hiding_nested_source_directories() {
    let root = policy_fixture_root("nested-exclusion").expect("fixture root should be creatable");
    fs::create_dir_all(root.join("vendor")).expect("root vendor directory should be creatable");
    fs::write(
        root.join("vendor/generated.rs"),
        "#[allow(dead_code)] fn generated() {}",
    )
    .expect("generated source should be writable");

    let nested = root.join("src/vendor");
    fs::create_dir_all(&nested).expect("nested vendor source should be creatable");
    fs::write(
        nested.join("mod.rs"),
        "#[path = \"payload.inc\"] mod payload;",
    )
    .expect("nested source should be writable");
    fs::write(nested.join("payload.inc"), "fn payload() {}")
        .expect("nested payload should be writable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("src/vendor/mod.rs"));
}

#[cfg(unix)]
#[test]
fn follows_internal_rust_symlinks_and_rejects_external_targets() {
    use std::os::unix::fs::symlink;

    let root = policy_fixture_root("symlinks").expect("fixture root should be creatable");
    let external = root
        .parent()
        .expect("fixture root should have a parent")
        .join(format!("external-{}", std::process::id()));
    fs::write(
        root.join("payload.inc"),
        "#[allow(dead_code)] fn internal() {}",
    )
    .expect("internal symlink target should be writable");
    fs::write(&external, "fn external() {}").expect("external symlink target should be writable");
    symlink("payload.inc", root.join("internal.rs")).expect("internal symlink should be creatable");
    symlink(&external, root.join("external.rs")).expect("external symlink should be creatable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 2);
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("payload.inc"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("outside canonical project root"))
    );
}

#[cfg(unix)]
#[test]
fn follows_internal_directory_symlinks_and_rejects_external_targets() {
    use std::os::unix::fs::symlink;

    let root = policy_fixture_root("directory-symlinks").expect("fixture root should be creatable");
    let internal = root.join("target/compiler-input");
    fs::create_dir_all(&internal).expect("internal target directory should be creatable");
    fs::write(
        internal.join("payload.rs"),
        "#[allow(dead_code)] fn internal() {}",
    )
    .expect("internal target source should be writable");
    symlink(&internal, root.join("src")).expect("internal directory symlink should be creatable");
    symlink(root.join("src"), internal.join("cycle"))
        .expect("directory symlink cycle should be creatable");

    let external = root
        .parent()
        .expect("fixture root should have a parent")
        .join(format!("external-directory-{}", std::process::id()));
    fs::create_dir_all(&external).expect("external target directory should be creatable");
    fs::write(external.join("payload.rs"), "fn external() {}")
        .expect("external target source should be writable");
    symlink(&external, root.join("external-src"))
        .expect("external directory symlink should be creatable");

    let violations = project_policy_violations(&root).expect("fixture project should be readable");
    assert_eq!(violations.len(), 2);
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("payload.rs"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("outside canonical project root"))
    );
}

/// Cargo reruns a copied tree's up-to-date test binary, whose compile-time
/// `CARGO_MANIFEST_DIR` still names the original tree, so the value Cargo
/// sets at run time wins.
fn project_root() -> PathBuf {
    env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from)
}

fn policy_fixture_root(name: &str) -> io::Result<PathBuf> {
    let root = project_root()
        .join("target/allow-policy-fixtures")
        .join(format!("{}-{name}", std::process::id()));
    fs::create_dir_all(&root)?;
    Ok(root)
}

fn write_fixture_lock(root: &Path, package_names: &[&str]) -> io::Result<()> {
    let mut lock = String::from("# This file is automatically @generated by Cargo.\nversion = 4\n");
    for package_name in package_names {
        lock.push_str("\n[[package]]\nname = \"");
        lock.push_str(package_name);
        lock.push_str("\"\nversion = \"0.1.0\"\n");
    }
    fs::write(root.join("Cargo.lock"), lock)
}

/// Writes a one-package fixture project with the given files and returns its
/// policy violations.
fn fixture_violations(name: &str, files: &[(&str, &str)]) -> io::Result<Vec<String>> {
    let root = policy_fixture_root(name)?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n[workspace.lints]\n[lints]\nworkspace = true\n"
        ),
    )?;
    write_fixture_lock(&root, &[name])?;
    for (path, source) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, source)?;
    }
    project_policy_violations(&root)
}

#[test]
fn project_sources_contain_no_policy_violations() {
    let root = project_root();
    let violations = project_policy_violations(&root).expect("project files should be readable");

    assert!(
        violations.is_empty(),
        "first-party sources break the source policy; each violation names its fix: {}",
        violations.join(", ")
    );
}

#[test]
fn mutation_tasks_preserve_workspace_and_base_resolution_contracts() {
    let root = project_root();
    let task_path = root.join(".config/mise/conf.d/20-rust.toml");
    let tasks =
        fs::read_to_string(&task_path).expect("the copied Rust task fragment should be readable");
    let Some(full) = task_definition(&tasks, "rust:mutants") else {
        panic!("the full Rust mutation task should exist");
    };
    let Some(diff) = task_definition(&tasks, "rust:mutants:diff") else {
        panic!("the diff Rust mutation task should exist");
    };

    assert!(
        full.contains("--test-workspace=true"),
        "the full mutation task must run every workspace test"
    );
    assert!(
        full.lines()
            .any(|line| line.contains("cargo mutants ") && line.contains("--output=.")),
        "the full mutation task must force cargo-mutants to write the report under the project root"
    );
    assert!(
        full.contains("sh scripts/verify-mutants.sh full mutants.out"),
        "the full mutation task must verify post-run outcome evidence"
    );
    for contract in [
        ". scripts/mutants-lock.sh",
        "acquire_mutants_task_lock full",
        "run_mutants_task_child",
        "trap mutants_task_release_lock EXIT",
    ] {
        assert!(
            full.contains(contract),
            "the full mutation task is missing transaction-lock contract {contract}"
        );
    }
    assert!(
        diff.contains("--test-workspace=true"),
        "the diff mutation task must run every workspace test"
    );
    assert!(
        diff.lines()
            .any(|line| line.contains("cargo mutants ") && line.contains("--output=.")),
        "the diff mutation task must force cargo-mutants to write the report under the project root"
    );
    assert!(
        diff.contains("sh scripts/verify-mutants.sh diff mutants.out"),
        "the diff mutation task must verify post-run outcome evidence"
    );
    for contract in [
        "refs/remotes/origin/main^{commit}",
        "refs/heads/main^{commit}",
        "$base_ref^{commit}",
        "git merge-base HEAD \"$base_commit\"",
        "git ls-files --others --exclude-standard",
        "git add -N <path>",
        "git diff --relative \"$merge_base\" --",
        ". scripts/mutants-lock.sh",
        "acquire_mutants_task_lock diff",
        "run_mutants_task_child",
        "mutants_task_release_lock",
    ] {
        assert!(
            diff.contains(contract),
            "the diff mutation task is missing base-resolution contract {contract}"
        );
    }
    for (lane, task) in [("full", full), ("diff", diff)] {
        let steps = [
            format!("acquire_mutants_task_lock {lane}"),
            "rm -rf mutants.out".to_owned(),
            "run_mutants_task_child cargo mutants ".to_owned(),
            format!("sh scripts/verify-mutants.sh {lane} mutants.out"),
        ];
        let positions: Vec<usize> = steps
            .iter()
            .map(|step| {
                task.find(step.as_str())
                    .unwrap_or_else(|| panic!("the {lane} mutation task is missing {step}"))
            })
            .collect();
        assert!(
            positions.is_sorted(),
            "the {lane} mutation task must clear the previous report under its lock, then run and verify cargo-mutants"
        );
    }
    let no_report = diff.find("if [ ! -d mutants.out ]; then").expect(
        "the diff mutation task must pass without a report when the diff selects no mutants",
    );
    let diff_run = diff
        .find("run_mutants_task_child cargo mutants ")
        .expect("the diff mutation task should run cargo-mutants");
    assert!(
        diff_run < no_report,
        "the diff mutation task must check for a report only after cargo-mutants runs"
    );
    let untracked_guard = diff
        .find("git ls-files --others --exclude-standard")
        .expect("the diff mutation task should inspect untracked files");
    let diff_capture = diff
        .find("git diff --relative \"$merge_base\" --")
        .expect("the diff mutation task should capture the merge-base diff");
    assert!(
        untracked_guard < diff_capture,
        "the untracked-file guard must run before the mutation diff is captured"
    );
}

fn task_definition<'a>(tasks: &'a str, name: &str) -> Option<&'a str> {
    let heading = format!("[tasks.\"{name}\"]");
    let (_, after_heading) = tasks.split_once(&heading)?;
    Some(
        after_heading
            .split_once("\n[tasks.")
            .map_or(after_heading, |(definition, _)| definition),
    )
}
