//! Reads obid's source with `syn` and prints the Brain's map of it as one JSON document.
//!
//! Usage: `obid-extract <repo root>`. Reads `core/crates/obid/src/**/*.rs` and `core/crates/obid/tests/*.rs`.

use proc_macro2::{Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Lit, Meta, Type};

// ---------- output (the JSON contract: keep the key names) ----------

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Out {
    files: Vec<FileOut>,
    modules: Vec<ModuleOut>,
    external_uses: Vec<ExternalOut>,
    routes: Vec<RouteOut>,
    phone_routes: Vec<PhoneRoute>,
    spawns: Vec<SpawnOut>,
    stored_files: Vec<StoredFile>,
    structs: Vec<StructOut>,
    consts: Vec<ConstOut>,
    fns: Vec<FnOut>,
    errors: Vec<ErrorOut>,
}

#[derive(Serialize)]
struct FileOut {
    path: String,
    module: String,
    lines: usize,
    doc: Option<String>,
}

#[derive(Serialize)]
struct ModuleOut {
    id: String,
    file: String,
    lines: usize,
    uses: Vec<String>,
}

#[derive(Serialize)]
struct ExternalOut {
    file: String,
    crates: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteOut {
    method: String,
    path: String,
    router: String,
    file: String,
    line: usize,
    handler: String,
    handler_file: Option<String>,
    handler_line: Option<usize>,
    handler_doc: Option<String>,
    extractors: Vec<String>,
    errors: Vec<RouteError>,
}

#[derive(Serialize, Clone)]
struct RouteError {
    status: String,
    message: String,
    line: usize,
}

#[derive(Serialize)]
struct PhoneRoute {
    method: String,
    pattern: String,
    line: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpawnOut {
    file: String,
    line: usize,
    kind: &'static str,
    in_fn: Option<String>,
    in_fn_line: Option<usize>,
    doc: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredFile {
    name: String,
    file: String,
    line: usize,
    in_fn: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StructOut {
    name: String,
    file: String,
    line: usize,
    rename_all: Option<String>,
    fields: Vec<FieldOut>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldOut {
    name: String,
    json: String,
    optional: bool,
    skip_if_none: bool,
    flatten: bool,
    /// The type's own name (`Tuning` for `crate::settings::Tuning`): what a `flatten` field brings in.
    ty: Option<String>,
}

#[derive(Serialize)]
struct ConstOut {
    name: String,
    file: String,
    line: usize,
    value: String,
}

#[derive(Serialize)]
struct FnOut {
    file: String,
    name: String,
    line: usize,
    test: bool,
    doc: Option<String>,
}

#[derive(Serialize)]
struct ErrorOut {
    file: String,
    message: String,
}

// ---------- input ----------

struct Input {
    path: String,
    /// `crate` for lib.rs/main.rs, else `jobs`, `brain::plan`, …
    module: String,
    src: String,
    /// An integration test file: only its fns are read.
    tests_only: bool,
}

impl Input {
    /// The top-level module id: `lib`/`main` for the crate roots, else the first segment.
    fn top(&self) -> String {
        if self.module == "crate" {
            Path::new(&self.path).file_stem().unwrap().to_string_lossy().into_owned()
        } else {
            self.module.split("::").next().unwrap().to_string()
        }
    }
}

const MARKERS: &[&str] = &["std::fs", "std::process", "tokio::process", "tokio::fs", "std::net"];
const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options", "trace", "any"];

/// A free fn a route's handler can resolve to.
struct FnDef {
    file: String,
    module: String,
    name: String,
    line: usize,
    doc: Option<String>,
    params: Vec<String>,
    errors: Vec<RouteError>,
}

struct RawRoute {
    method: String,
    path: String,
    router: String,
    file: String,
    line: usize,
    handler: String,
}

fn line(span: proc_macro2::Span) -> usize {
    span.start().line
}

fn ident_name(i: &syn::Ident) -> String {
    let s = i.to_string();
    s.strip_prefix("r#").map(str::to_string).unwrap_or(s)
}

/// The first non-empty doc line of these attributes (`///` outer or `//!` inner).
fn doc(attrs: &[Attribute]) -> Option<String> {
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let Meta::NameValue(nv) = &a.meta {
            if let Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) = &nv.value {
                let v = s.value();
                if let Some(first) = v.lines().map(str::trim).find(|l| !l.is_empty()) {
                    return Some(first.to_string());
                }
            }
        }
    }
    None
}

fn cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("cfg") {
            return false;
        }
        let Meta::List(l) = &a.meta else { return false };
        let mut idents = Vec::new();
        collect_idents(l.tokens.clone(), &mut idents);
        // ponytail: `not(test)` anywhere means "not a test item"; good enough for obid's cfgs.
        idents.iter().any(|i| i == "test") && !idents.iter().any(|i| i == "not")
    })
}

fn collect_idents(ts: TokenStream, out: &mut Vec<String>) {
    for t in ts {
        match t {
            TokenTree::Ident(i) => out.push(i.to_string()),
            TokenTree::Group(g) => collect_idents(g.stream(), out),
            _ => {}
        }
    }
}

fn has_test_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

fn item_attrs(item: &syn::Item) -> &[Attribute] {
    use syn::Item::*;
    match item {
        Const(i) => &i.attrs,
        Enum(i) => &i.attrs,
        ExternCrate(i) => &i.attrs,
        Fn(i) => &i.attrs,
        ForeignMod(i) => &i.attrs,
        Impl(i) => &i.attrs,
        Macro(i) => &i.attrs,
        Mod(i) => &i.attrs,
        Static(i) => &i.attrs,
        Struct(i) => &i.attrs,
        Trait(i) => &i.attrs,
        TraitAlias(i) => &i.attrs,
        Type(i) => &i.attrs,
        Union(i) => &i.attrs,
        Use(i) => &i.attrs,
        _ => &[],
    }
}

/// Tokens as written, whitespace-normalised: `State<Arc<Shared>>`, `&'static str`, `(A, B)`.
fn norm(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'' || c == '"';
    let mut out = String::new();
    let mut in_str = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c.is_whitespace() {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if let (Some(p), Some(n)) = (out.chars().last(), next) {
                if word(p) && word(*n) {
                    out.push(' ');
                }
            }
        } else if c == ',' {
            out.push_str(", ");
        } else if c == '-' && chars.get(i + 1) == Some(&'>') {
            out.push_str(" -> ");
            i += 1;
        } else {
            out.push(c);
        }
        i += 1;
    }
    out.replace("  ", " ").trim().to_string()
}

fn text<T: ToTokens>(t: &T) -> String {
    norm(&t.to_token_stream().to_string())
}

fn path_segs(p: &syn::Path) -> Vec<String> {
    p.segments.iter().map(|s| ident_name(&s.ident)).collect()
}

fn is_colon(t: Option<&TokenTree>, spacing: Option<Spacing>) -> bool {
    matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == ':' && spacing.is_none_or(|s| p.spacing() == s))
}

/// Paths (`a :: b :: c`) and string literals inside a macro's token stream.
fn scan_tokens(ts: TokenStream, paths: &mut Vec<Vec<String>>, lits: &mut Vec<(String, usize)>) {
    let toks: Vec<TokenTree> = ts.into_iter().collect();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            TokenTree::Group(g) => {
                scan_tokens(g.stream(), paths, lits);
                i += 1;
            }
            TokenTree::Literal(l) => {
                if let Ok(s) = syn::parse2::<syn::LitStr>(TokenTree::Literal(l.clone()).into()) {
                    lits.push((s.value(), line(l.span())));
                }
                i += 1;
            }
            TokenTree::Ident(id) => {
                let mut segs = vec![id.to_string()];
                let mut j = i + 1;
                while is_colon(toks.get(j), Some(Spacing::Joint)) && is_colon(toks.get(j + 1), None) {
                    match toks.get(j + 2) {
                        Some(TokenTree::Ident(next)) => {
                            segs.push(next.to_string());
                            j += 3;
                        }
                        _ => break,
                    }
                }
                if segs.len() > 1 {
                    paths.push(segs);
                }
                i = j;
            }
            _ => i += 1,
        }
    }
}

fn stored_name(s: &str) -> Option<String> {
    let ok = |seg: &str| {
        !seg.is_empty()
            && seg.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            && (seg.strip_suffix(".json").or_else(|| seg.strip_suffix(".jsonl")))
                .is_some_and(|stem| !stem.is_empty())
    };
    let last = s.rsplit('/').next()?;
    let all_path_chars = s.chars().all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c));
    (ok(last) && all_path_chars).then(|| s.to_string())
}

fn rename(name: &str, rule: Option<&str>) -> String {
    match rule {
        Some("camelCase") => {
            let mut out = String::new();
            let mut up = false;
            for c in name.chars() {
                if c == '_' {
                    up = true;
                } else if up {
                    out.extend(c.to_uppercase());
                    up = false;
                } else {
                    out.push(c);
                }
            }
            out
        }
        Some("kebab-case") => name.replace('_', "-"),
        Some("lowercase") => name.to_lowercase(),
        Some("UPPERCASE") | Some("SCREAMING_SNAKE_CASE") => name.to_uppercase(),
        _ => name.to_string(),
    }
}

/// `#[serde(...)]` keys with their string value: `rename = "x"` and `rename(serialize = "x")` both give `("rename", Some("x"))`.
fn serde_keys(attrs: &[Attribute]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for a in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        let Meta::List(l) = &a.meta else { continue };
        let toks: Vec<TokenTree> = l.tokens.clone().into_iter().collect();
        let lit = |t: Option<&TokenTree>| match t {
            Some(TokenTree::Literal(l)) => syn::parse2::<syn::LitStr>(TokenTree::Literal(l.clone()).into())
                .ok()
                .map(|s| s.value()),
            _ => None,
        };
        let mut i = 0;
        while i < toks.len() {
            if let TokenTree::Ident(k) = &toks[i] {
                let key = k.to_string();
                match toks.get(i + 1) {
                    Some(TokenTree::Punct(p)) if p.as_char() == '=' => {
                        out.push((key, lit(toks.get(i + 2))));
                        i += 3;
                        continue;
                    }
                    Some(TokenTree::Group(g)) => {
                        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                        let ser = inner.windows(3).find_map(|w| match (&w[0], &w[1]) {
                            (TokenTree::Ident(i), TokenTree::Punct(p)) if i == "serialize" && p.as_char() == '=' => {
                                lit(Some(&w[2]))
                            }
                            _ => None,
                        });
                        out.push((key, ser));
                        i += 2;
                        continue;
                    }
                    _ => out.push((key, None)),
                }
            }
            i += 1;
        }
    }
    out
}

fn derives_serialize(attrs: &[Attribute]) -> bool {
    attrs.iter().filter(|a| a.path().is_ident("derive")).any(|a| {
        let Meta::List(l) = &a.meta else { return false };
        let mut idents = Vec::new();
        collect_idents(l.tokens.clone(), &mut idents);
        idents.iter().any(|i| i == "Serialize")
    })
}

fn str_lit(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) => Some(s.value()),
        // `"x".to_string()`, `"x".into()`, `"x".to_owned()`
        Expr::MethodCall(m) if m.args.is_empty() => str_lit(&m.receiver),
        Expr::Paren(p) => str_lit(&p.expr),
        Expr::Reference(r) => str_lit(&r.expr),
        _ => None,
    }
}

/// `ApiError::new(StatusCode::X, …)` calls in a fn body.
#[derive(Default)]
struct ErrorFinder {
    found: Vec<RouteError>,
}

impl<'ast> Visit<'ast> for ErrorFinder {
    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let Expr::Path(p) = &*c.func {
            let segs = path_segs(&p.path);
            if segs.len() >= 2 && segs[segs.len() - 2] == "ApiError" && c.args.len() == 2 {
                let status = match &c.args[0] {
                    Expr::Path(s) => s.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
                    other => text(other),
                };
                let msg = &c.args[1];
                let message = str_lit(msg)
                    .or_else(|| match msg {
                        Expr::Macro(m) if m.mac.path.is_ident("format") => {
                            let (mut paths, mut lits) = (Vec::new(), Vec::new());
                            scan_tokens(m.mac.tokens.clone(), &mut paths, &mut lits);
                            lits.into_iter().next().map(|(s, _)| s)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| text(msg));
                self.found.push(RouteError { status, message, line: line(segs_span(&p.path)) });
            }
        }
        visit::visit_expr_call(self, c);
    }
}

fn segs_span(p: &syn::Path) -> proc_macro2::Span {
    p.segments.first().map(|s| s.ident.span()).unwrap_or_else(proc_macro2::Span::call_site)
}

/// Expands `get(a).post(b)` into `[("GET", a), ("POST", b)]`; `None` when it isn't a method router.
fn route_methods(e: &Expr, out: &mut Vec<(String, Expr)>) -> bool {
    match e {
        Expr::MethodCall(m) => {
            if !route_methods(&m.receiver, out) {
                return false;
            }
            let n = m.method.to_string();
            if METHODS.contains(&n.as_str()) && m.args.len() == 1 {
                out.push((n.to_uppercase(), m.args[0].clone()));
            }
            true
        }
        Expr::Call(c) => {
            let Expr::Path(p) = &*c.func else { return false };
            let Some(last) = p.path.segments.last() else { return false };
            let n = last.ident.to_string();
            if METHODS.contains(&n.as_str()) && c.args.len() == 1 {
                out.push((n.to_uppercase(), c.args[0].clone()));
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

struct FnCtx {
    name: String,
    line: usize,
    doc: Option<String>,
}

struct Cx<'a> {
    file: String,
    module: String,
    top: String,
    deps: &'a BTreeSet<String>,
    known: &'a BTreeSet<String>,
    /// Bare `spawn`/`interval`/`spawn_blocking` → the path it was imported from.
    imports: HashMap<String, String>,
    test_depth: usize,
    fn_stack: Vec<FnCtx>,
    impl_stack: Vec<String>,
    uses: BTreeSet<String>,
    ext: BTreeSet<String>,
    out: &'a mut Out,
    defs: &'a mut Vec<FnDef>,
    raw_routes: &'a mut Vec<RawRoute>,
}

impl Cx<'_> {
    fn live(&self) -> bool {
        self.test_depth == 0
    }

    fn record_path(&mut self, segs: &[String], from_use: bool) {
        let Some(first) = segs.first() else { return };
        // A crate root names its own modules bare (`jobs::x`); main.rs names the lib's as `obid::x`.
        if self.module == "crate" && first != "crate" && self.known.contains(first) && (segs.len() >= 2 || from_use) {
            self.uses.insert(first.clone());
            return;
        }
        if first == "crate" || (first == "obid" && self.top == "main") {
            if let Some(m) = segs.get(1) {
                let id = if self.known.contains(m) { m.clone() } else { "lib".to_string() };
                if id != self.top {
                    self.uses.insert(id);
                }
            }
            return;
        }
        if (segs.len() >= 2 || from_use) && (self.deps.contains(first) || ["std", "core", "alloc"].contains(&first.as_str())) {
            self.ext.insert(first.clone());
            if segs.len() >= 2 {
                let two = format!("{}::{}", segs[0], segs[1]);
                if MARKERS.contains(&two.as_str()) {
                    self.ext.insert(two);
                }
            }
        }
    }

    fn use_tree(&mut self, prefix: &mut Vec<String>, tree: &syn::UseTree) {
        use syn::UseTree::*;
        let leaf = |this: &mut Self, prefix: &Vec<String>, name: String, alias: String| {
            let mut full = prefix.clone();
            full.push(name.clone());
            if ["spawn", "spawn_blocking", "interval"].contains(&name.as_str()) {
                this.imports.insert(alias, full.join("::"));
            }
            this.record_path(&full, true);
        };
        match tree {
            Path(p) => {
                prefix.push(ident_name(&p.ident));
                self.use_tree(prefix, &p.tree);
                prefix.pop();
            }
            Name(n) => {
                let name = ident_name(&n.ident);
                if name == "self" {
                    self.record_path(prefix, true);
                } else {
                    leaf(self, prefix, name.clone(), name);
                }
            }
            Rename(r) => leaf(self, prefix, ident_name(&r.ident), ident_name(&r.rename)),
            Glob(_) => self.record_path(prefix, true),
            Group(g) => {
                for t in &g.items {
                    self.use_tree(prefix, t);
                }
            }
        }
    }

    fn stored(&mut self, value: &str, at: usize) {
        if let Some(name) = stored_name(value) {
            let in_fn = self.fn_stack.last().map(|f| f.name.clone());
            self.out.stored_files.push(StoredFile { name, file: self.file.clone(), line: at, in_fn });
        }
    }

    fn spawn(&mut self, kind: &'static str, at: usize) {
        let f = self.fn_stack.last();
        self.out.spawns.push(SpawnOut {
            file: self.file.clone(),
            line: at,
            kind,
            in_fn: f.map(|f| f.name.clone()),
            in_fn_line: f.map(|f| f.line),
            doc: f.and_then(|f| f.doc.clone()),
        });
    }

    /// Records a fn (free or method) and walks its body with it on the stack.
    fn enter_fn(&mut self, name: String, attrs: &[Attribute], sig: &syn::Signature, walk: impl FnOnce(&mut Self)) {
        let test = has_test_attr(attrs);
        let test_region = test || cfg_test(attrs);
        let at = line(sig.ident.span());
        let d = doc(attrs);
        self.out.fns.push(FnOut { file: self.file.clone(), name: name.clone(), line: at, test, doc: d.clone() });
        if test_region {
            self.test_depth += 1;
        }
        self.fn_stack.push(FnCtx { name, line: at, doc: d });
        walk(self);
        self.fn_stack.pop();
        if test_region {
            self.test_depth -= 1;
        }
    }
}

impl<'ast> Visit<'ast> for Cx<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let t = cfg_test(item_attrs(item));
        if t {
            self.test_depth += 1;
        }
        visit::visit_item(self, item);
        if t {
            self.test_depth -= 1;
        }
    }

    fn visit_attribute(&mut self, a: &'ast Attribute) {
        // The attribute's own path (`tokio::main`) is a use; its arguments (docs, serde keys) are not code.
        if self.live() {
            let segs = path_segs(a.path());
            self.record_path(&segs, false);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let name = ident_name(&f.sig.ident);
        if self.live() && !has_test_attr(&f.attrs) && !cfg_test(&f.attrs) {
            let mut finder = ErrorFinder::default();
            finder.visit_block(&f.block);
            self.defs.push(FnDef {
                file: self.file.clone(),
                module: self.module.clone(),
                name: name.clone(),
                line: line(f.sig.ident.span()),
                doc: doc(&f.attrs),
                params: f
                    .sig
                    .inputs
                    .iter()
                    .filter_map(|a| match a {
                        syn::FnArg::Typed(t) => Some(text(&t.ty)),
                        syn::FnArg::Receiver(_) => None,
                    })
                    .collect(),
                errors: finder.found,
            });
        }
        self.enter_fn(name, &f.attrs, &f.sig, |this| visit::visit_item_fn(this, f));
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let ty = match &*i.self_ty {
            Type::Path(p) => p.path.segments.last().map(|s| ident_name(&s.ident)).unwrap_or_default(),
            other => text(other),
        };
        self.impl_stack.push(ty);
        visit::visit_item_impl(self, i);
        self.impl_stack.pop();
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        self.impl_stack.push(ident_name(&t.ident));
        visit::visit_item_trait(self, t);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        let name = format!("{}::{}", self.impl_stack.last().cloned().unwrap_or_default(), ident_name(&f.sig.ident));
        self.enter_fn(name, &f.attrs, &f.sig, |this| visit::visit_impl_item_fn(this, f));
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        let name = format!("{}::{}", self.impl_stack.last().cloned().unwrap_or_default(), ident_name(&f.sig.ident));
        self.enter_fn(name, &f.attrs, &f.sig, |this| visit::visit_trait_item_fn(this, f));
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        if self.live() {
            self.use_tree(&mut Vec::new(), &u.tree);
        }
    }

    fn visit_path(&mut self, p: &'ast syn::Path) {
        if self.live() {
            let segs = path_segs(p);
            self.record_path(&segs, false);
        }
        visit::visit_path(self, p);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        visit::visit_macro(self, m);
        if !self.live() {
            return;
        }
        let (mut paths, mut lits) = (Vec::new(), Vec::new());
        scan_tokens(m.tokens.clone(), &mut paths, &mut lits);
        for p in paths {
            self.record_path(&p, false);
        }
        for (s, at) in lits {
            self.stored(&s, at);
        }
    }

    fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
        if self.live() {
            self.stored(&s.value(), line(s.span()));
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        // The receiver first, so a router's routes come out in source order.
        visit::visit_expr_method_call(self, m);
        if self.live() {
            let name = m.method.to_string();
            if name == "route" && m.args.len() == 2 {
                if let Some(path) = str_lit(&m.args[0]) {
                    let mut methods = Vec::new();
                    if route_methods(&m.args[1], &mut methods) {
                        for (method, handler) in methods {
                            let handler = match &handler {
                                Expr::Path(p) => path_segs(&p.path).join("::"),
                                other => text(other).chars().take(120).collect(),
                            };
                            self.raw_routes.push(RawRoute {
                                method,
                                path: path.clone(),
                                router: self.module.clone(),
                                file: self.file.clone(),
                                line: line(m.method.span()),
                                handler,
                            });
                        }
                    }
                }
            }
            // `std::thread::Builder::new()….spawn(…)`
            if name == "spawn" && m.receiver.to_token_stream().to_string().contains("thread :: Builder") {
                self.spawn("thread::spawn", line(m.method.span()));
            }
        }
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if self.live() {
            if let Expr::Path(p) = &*c.func {
                let segs = path_segs(&p.path);
                let full = if segs.len() == 1 {
                    self.imports.get(&segs[0]).cloned().unwrap_or_else(|| segs[0].clone())
                } else {
                    segs.join("::")
                };
                let kind = match full.as_str() {
                    "tokio::spawn" | "tokio::task::spawn" | "task::spawn" => Some("tokio::spawn"),
                    "tokio::task::spawn_blocking" | "task::spawn_blocking" | "spawn_blocking" => Some("spawn_blocking"),
                    "std::thread::spawn" | "thread::spawn" => Some("thread::spawn"),
                    "tokio::time::interval" | "time::interval" | "interval" => Some("interval"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    self.spawn(kind, line(segs_span(&p.path)));
                }
            }
        }
        visit::visit_expr_call(self, c);
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        if self.live() {
            let name = ident_name(&c.ident);
            let is_str = matches!(&*c.ty, Type::Reference(r) if matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")));
            if let (true, Some(value)) = (is_str, str_lit(&c.expr)) {
                self.out.consts.push(ConstOut { name: name.clone(), file: self.file.clone(), line: line(c.ident.span()), value });
            }
            if name == "ROUTES" && self.module == "gateway" {
                if let Expr::Reference(r) = &*c.expr {
                    if let Expr::Array(a) = &*r.expr {
                        for e in &a.elems {
                            if let Expr::Tuple(t) = e {
                                let parts: Vec<_> = t.elems.iter().filter_map(str_lit).collect();
                                if let [method, pattern] = parts.as_slice() {
                                    let at = t.elems.first().map(|e| match e {
                                        Expr::Lit(l) => line(l.lit.span()),
                                        _ => 0,
                                    });
                                    self.out.phone_routes.push(PhoneRoute {
                                        method: method.clone(),
                                        pattern: pattern.clone(),
                                        line: at.unwrap_or(0),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        visit::visit_item_const(self, c);
    }

    fn visit_item_struct(&mut self, s: &'ast syn::ItemStruct) {
        if self.live() && derives_serialize(&s.attrs) {
            let rename_all = serde_keys(&s.attrs).into_iter().find(|(k, _)| k == "rename_all").and_then(|(_, v)| v);
            let mut fields = Vec::new();
            for f in &s.fields {
                let Some(id) = &f.ident else { continue };
                let keys = serde_keys(&f.attrs);
                let has = |k: &str| keys.iter().any(|(key, _)| key == k);
                if has("skip") || has("skip_serializing") {
                    continue;
                }
                let name = ident_name(id);
                let json = keys
                    .iter()
                    .find(|(k, v)| k == "rename" && v.is_some())
                    .and_then(|(_, v)| v.clone())
                    .unwrap_or_else(|| rename(&name, rename_all.as_deref()));
                let optional = matches!(&f.ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Option"));
                fields.push(FieldOut {
                    name,
                    json,
                    optional,
                    skip_if_none: has("skip_serializing_if"),
                    flatten: has("flatten"),
                    ty: match &f.ty {
                        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
                        _ => None,
                    },
                });
            }
            self.out.structs.push(StructOut {
                name: ident_name(&s.ident),
                file: self.file.clone(),
                line: line(s.ident.span()),
                rename_all,
                fields,
            });
        }
        visit::visit_item_struct(self, s);
    }
}

fn run(inputs: &[Input], deps: &BTreeSet<String>) -> Out {
    let known: BTreeSet<String> = inputs.iter().filter(|i| !i.tests_only && i.module != "crate").map(Input::top).collect();
    let mut out = Out::default();
    let mut defs = Vec::new();
    let mut raw_routes = Vec::new();
    // top-level id → (first file, lines, uses)
    let mut modules: BTreeMap<String, (String, usize, BTreeSet<String>)> = BTreeMap::new();
    for inp in inputs {
        let lines = inp.src.lines().count();
        let parsed = syn::parse_file(&inp.src);
        if !inp.tests_only {
            out.files.push(FileOut {
                path: inp.path.clone(),
                module: inp.module.clone(),
                lines,
                doc: parsed.as_ref().ok().and_then(|f| doc(&f.attrs)),
            });
        }
        let file = match parsed {
            Ok(f) => f,
            Err(e) => {
                out.errors.push(ErrorOut { file: inp.path.clone(), message: format!("line {}: {e}", e.span().start().line) });
                continue;
            }
        };
        let top = inp.top();
        let mut cx = Cx {
            file: inp.path.clone(),
            module: inp.module.clone(),
            top: top.clone(),
            deps,
            known: &known,
            imports: HashMap::new(),
            test_depth: usize::from(inp.tests_only),
            fn_stack: Vec::new(),
            impl_stack: Vec::new(),
            uses: BTreeSet::new(),
            ext: BTreeSet::new(),
            out: &mut out,
            defs: &mut defs,
            raw_routes: &mut raw_routes,
        };
        cx.visit_file(&file);
        let (uses, ext) = (cx.uses, cx.ext);
        if inp.tests_only {
            continue;
        }
        if !ext.is_empty() {
            out.external_uses.push(ExternalOut { file: inp.path.clone(), crates: ext.into_iter().collect() });
        }
        let m = modules.entry(top.clone()).or_insert_with(|| (inp.path.clone(), 0, BTreeSet::new()));
        // The module's own file (`jobs.rs`, `brain/mod.rs`) stands for it, not its first submodule.
        if inp.module == top {
            m.0 = inp.path.clone();
        }
        m.1 += lines;
        m.2.extend(uses);
    }
    for (id, (file, lines, uses)) in modules {
        out.modules.push(ModuleOut { id, file, lines, uses: uses.into_iter().collect() });
    }
    for r in raw_routes {
        let def = resolve(&defs, &r);
        out.routes.push(RouteOut {
            method: r.method,
            path: r.path,
            router: r.router,
            file: r.file,
            line: r.line,
            handler: r.handler,
            handler_file: def.map(|d| d.file.clone()),
            handler_line: def.map(|d| d.line),
            handler_doc: def.and_then(|d| d.doc.clone()),
            extractors: def.map(|d| d.params.clone()).unwrap_or_default(),
            errors: def.map(|d| d.errors.clone()).unwrap_or_default(),
        });
    }
    out
}

fn resolve<'a>(defs: &'a [FnDef], r: &RawRoute) -> Option<&'a FnDef> {
    let segs: Vec<&str> = r.handler.split("::").collect();
    let (name, mods) = segs.split_last()?;
    if mods.is_empty() {
        return defs.iter().find(|d| d.name == *name && d.file == r.file).or_else(|| defs.iter().find(|d| d.name == *name));
    }
    let module = match mods {
        ["crate"] => "crate".to_string(),
        ["crate", rest @ ..] => rest.join("::"),
        rest => rest.join("::"),
    };
    let suffix = format!("::{module}");
    defs.iter()
        .find(|d| d.name == *name && d.module == module)
        .or_else(|| defs.iter().find(|d| d.name == *name && d.module.ends_with(&suffix)))
}

/// Dependency names (as written in code) from obid's Cargo.toml.
fn deps(cargo_toml: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut in_deps = false;
    for l in cargo_toml.lines().map(str::trim) {
        if let Some(header) = l.strip_prefix('[') {
            let header = header.trim_end_matches(']');
            in_deps = header.ends_with("dependencies");
            if let Some(name) = header.rsplit_once("dependencies.").map(|(_, n)| n) {
                out.insert(name.replace('-', "_"));
            }
            continue;
        }
        if in_deps && !l.starts_with('#') {
            if let Some(key) = l.split(['=', '.', ' ']).next().filter(|k| !k.is_empty()) {
                out.insert(key.replace('-', "_"));
            }
        }
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn module_of(rel_to_src: &Path) -> String {
    let mut parts: Vec<String> = rel_to_src.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".rs").to_string();
    }
    if parts.len() > 1 && parts.last().is_some_and(|l| l == "mod") {
        parts.pop();
    }
    if parts.len() == 1 && (parts[0] == "lib" || parts[0] == "main") {
        return "crate".to_string();
    }
    parts.join("::")
}

fn main() {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: obid-extract <repo root>");
        std::process::exit(2);
    };
    let root = Path::new(&root);
    let krate = root.join("core/crates/obid");
    let src_dir = krate.join("src");
    let mut src_files = Vec::new();
    walk(&src_dir, &mut src_files);
    src_files.sort();
    let mut test_files: Vec<_> = std::fs::read_dir(krate.join("tests"))
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "rs")).collect())
        .unwrap_or_default();
    test_files.sort();

    let mut out_errors = Vec::new();
    let mut inputs = Vec::new();
    for (p, tests_only) in src_files.iter().map(|p| (p, false)).chain(test_files.iter().map(|p| (p, true))) {
        let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy().into_owned();
        match std::fs::read_to_string(p) {
            Ok(src) => inputs.push(Input {
                module: if tests_only { String::new() } else { module_of(p.strip_prefix(&src_dir).unwrap()) },
                path: rel,
                src,
                tests_only,
            }),
            Err(e) => out_errors.push(ErrorOut { file: rel, message: e.to_string() }),
        }
    }
    let deps = deps(&std::fs::read_to_string(krate.join("Cargo.toml")).unwrap_or_default());
    let mut out = run(&inputs, &deps);
    out.errors.extend(out_errors);
    if inputs.is_empty() {
        out.errors.push(ErrorOut { file: src_dir.to_string_lossy().into_owned(), message: "no .rs files found".into() });
    }
    let stdout = std::io::stdout().lock();
    serde_json::to_writer(stdout, &out).expect("write JSON to stdout");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(module: &str, src: &str) -> Out {
        let path = format!("core/crates/obid/src/{}.rs", module.replace("::", "/"));
        let deps: BTreeSet<String> = ["axum", "serde_json", "tokio"].iter().map(|s| s.to_string()).collect();
        run(&[Input { path, module: module.into(), src: src.into(), tests_only: false }], &deps)
    }

    #[test]
    fn route_chain_expands_per_method_and_resolves() {
        let out = one(
            "api",
            r#"
            /// The status.
            async fn status(State(s): State<Arc<Shared>>, Path(id): Path<String>) -> Result<(), ApiError> {
                Err(ApiError::new(StatusCode::NOT_FOUND, "No such job."))?;
                Err(ApiError::new(StatusCode::BAD_REQUEST, format!("bad {id}")))
            }
            fn router() -> Router {
                Router::new()
                    .route("/v1/status", get(status))
                    .route("/v1/x", axum::routing::patch(crate::a::change).delete(crate::a::remove))
            }
            "#,
        );
        let got: Vec<_> = out.routes.iter().map(|r| (r.method.as_str(), r.path.as_str(), r.handler.as_str())).collect();
        assert_eq!(
            got,
            [("GET", "/v1/status", "status"), ("PATCH", "/v1/x", "crate::a::change"), ("DELETE", "/v1/x", "crate::a::remove")]
        );
        let s = &out.routes[0];
        assert_eq!(s.handler_line, Some(3));
        assert_eq!(s.handler_doc.as_deref(), Some("The status."));
        assert_eq!(s.extractors, ["State<Arc<Shared>>", "Path<String>"]);
        assert_eq!(s.errors[0].status, "NOT_FOUND");
        assert_eq!(s.errors[0].message, "No such job.");
        assert_eq!(s.errors[1].message, "bad {id}");
        assert!(out.routes[1].handler_file.is_none());
    }

    #[test]
    fn rename_all_and_field_attrs() {
        let out = one(
            "jobs",
            r#"
            #[derive(Debug, Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Job {
                job_id: String,
                #[serde(rename = "kind")] job_kind: String,
                #[serde(skip_serializing_if = "Option::is_none")] max_minutes: Option<u32>,
                #[serde(flatten)] extra: Extra,
                #[serde(skip)] secret: String,
            }
            #[derive(Deserialize)] struct NotOut { a: u8 }
            "#,
        );
        assert_eq!(out.structs.len(), 1);
        let s = &out.structs[0];
        assert_eq!(s.rename_all.as_deref(), Some("camelCase"));
        let json: Vec<_> = s.fields.iter().map(|f| f.json.as_str()).collect();
        assert_eq!(json, ["jobId", "kind", "maxMinutes", "extra"]);
        assert!(s.fields[2].optional && s.fields[2].skip_if_none);
        assert!(s.fields[3].flatten);
        assert_eq!(s.fields[3].ty.as_deref(), Some("Extra"));
        assert_eq!(rename("a_b_c", Some("kebab-case")), "a-b-c");
    }

    #[test]
    fn cfg_test_is_skipped_but_tests_are_listed() {
        let out = one(
            "jobs",
            r#"
            const LIVE: &str = "live.json";
            fn work() { tokio::spawn(async {}); }
            #[cfg(test)]
            mod tests {
                const HIDDEN: &str = "hidden.json";
                fn helper() { tokio::spawn(async {}); let _ = crate::brain::x(); }
                #[tokio::test]
                async fn it_works() { let _ = Router::new().route("/t", get(helper)); }
            }
            "#,
        );
        assert_eq!(out.consts.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["LIVE"]);
        assert_eq!(out.spawns.len(), 1);
        assert_eq!(out.spawns[0].in_fn.as_deref(), Some("work"));
        assert!(out.routes.is_empty());
        assert_eq!(out.stored_files.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["live.json"]);
        assert!(out.modules[0].uses.is_empty());
        let t = out.fns.iter().find(|f| f.name == "it_works").unwrap();
        assert!(t.test);
    }

    #[test]
    fn crate_uses_inside_macro_tokens() {
        let src = r#"
            use crate::{api::ApiError, jobs};
            impl Thing {
                fn go(&self) {
                    let v = serde_json::json!({ "a": crate::sphere::link::x(), "f": "vault/secrets.json" });
                    tracing::info!("{}", crate::brain::plan::name());
                    std::fs::write("x", "y").ok();
                }
            }
        "#;
        let deps: BTreeSet<String> = ["serde_json", "tracing"].iter().map(|s| s.to_string()).collect();
        let inputs = ["api", "brain", "sphere", "jobs", "gateway"]
            .iter()
            .map(|m| Input {
                path: format!("src/{m}.rs"),
                module: m.to_string(),
                src: if *m == "gateway" { src.into() } else { String::new() },
                tests_only: false,
            })
            .collect::<Vec<_>>();
        let out = run(&inputs, &deps);
        let g = out.modules.iter().find(|m| m.id == "gateway").unwrap();
        assert_eq!(g.uses, ["api", "brain", "jobs", "sphere"]);
        let ext = &out.external_uses.iter().find(|e| e.file == "src/gateway.rs").unwrap().crates;
        assert_eq!(ext, &["serde_json", "std", "std::fs", "tracing"]);
        assert_eq!(out.stored_files[0].name, "vault/secrets.json");
        assert_eq!(out.stored_files[0].in_fn.as_deref(), Some("Thing::go"));
        assert_eq!(out.fns[0].name, "Thing::go");
    }
}
