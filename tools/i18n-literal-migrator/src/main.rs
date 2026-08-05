use anyhow::{Context, Result, bail};
use clap::Parser;
use proc_macro2::Span;
use quote::quote;
use std::{fs, path::{Path, PathBuf}};
use syn::{File, Expr, Lit, spanned::Spanned, visit_mut::VisitMut};

#[derive(Parser, Debug)]
#[command(about = "Wrap Rust string literals in rust-i18n t! macros")]
struct Args {
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    #[arg(short = 'n', long)]
    check: bool,
}

struct Migrator {
    replacements: Vec<(Span, String)>,
}

fn output_macro(path: &syn::Path) -> bool {
    path.segments.last().is_some_and(|segment| matches!(segment.ident.to_string().as_str(), "print" | "println" | "eprint" | "eprintln"))
}

impl Migrator {
    fn new() -> Self { Self { replacements: Vec::new() } }
}

impl VisitMut for Migrator {
    fn visit_attribute_mut(&mut self, _attribute: &mut syn::Attribute) {}

    fn visit_expr_macro_mut(&mut self, expr: &mut syn::ExprMacro) {
        self.visit_macro_mut(&mut expr.mac);
    }

    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        let syn::Expr::Path(path) = call.func.as_ref() else { return; };
        let Some(segment) = path.path.segments.last() else { return; };
        if !matches!(segment.ident.to_string().as_str(), "tr" | "effect_name" | "effect_category" | "effect_param_label") {
            return;
        }
        let Some(Expr::Lit(argument)) = call.args.first() else { return; };
        let Lit::Str(lit) = &argument.lit else { return; };
        if call.args.len() != 1 { return; }
        let replacement: Expr = syn::parse_quote!(t!(#lit));
        self.replacements.push((call.span(), quote!(#replacement).to_string()));
    }

    fn visit_macro_mut(&mut self, mac: &mut syn::Macro) {
        if !output_macro(&mac.path) {
            return;
        }
        let mut tokens = mac.tokens.clone().into_iter();
        let Some(proc_macro2::TokenTree::Literal(first_token)) = tokens.next() else { return; };
        let Ok(lit) = syn::parse_str::<syn::LitStr>(&first_token.to_string()) else { return; };
        let _rest_tokens: proc_macro2::TokenStream = tokens.collect();
        if lit.value() == "{}" { return; }

        let mut key = String::new();
        let mut generated_args = Vec::new();
        let value = lit.value();
        let mut rest = value.split('{');
        key.push_str(rest.next().unwrap_or_default());
        for (index, part) in rest.enumerate() {
            let Some((name, tail)) = part.split_once('}') else { key.push('{'); key.push_str(part); continue; };
            let arg_name = name.trim();
            if arg_name.is_empty() || arg_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                let slot = format!("arg{index}");
                key.push_str(&format!("%{{{slot}}}"));
                if !arg_name.is_empty() {
                    if let Ok(expr) = syn::parse_str::<Expr>(arg_name) {
                        generated_args.push((slot, expr));
                    }
                }
            } else { key.push('{'); key.push_str(name); key.push('}'); }
            key.push_str(tail);
        }
        let mut translated: Expr = syn::parse_quote!(t!(#key));
        if !generated_args.is_empty() {
            let pairs = generated_args.iter().map(|(name, expr)| { let ident = syn::Ident::new(name, Span::call_site()); quote!(#ident = format!("{}", #expr)) });
            translated = syn::parse_quote!(t!(#key, #(#pairs),*));
        }
        let path = &mac.path;
        let replacement = quote!(#path!("{}", #translated));
        let span = mac.path.span().join(mac.delimiter.span().close()).unwrap_or_else(|| mac.path.span());
        self.replacements.push((span, replacement.to_string()));
    }

    fn visit_expr_lit_mut(&mut self, _expr: &mut syn::ExprLit) {}
}

fn line_offsets(source: &str) -> Vec<usize> {
    std::iter::once(0).chain(source.match_indices('\n').map(|(i, _)| i + 1)).collect()
}

fn offset(source: &str, offsets: &[usize], line: usize, column: usize) -> usize {
    let line_start = offsets[line.saturating_sub(1)];
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |end| line_start + end);
    let line_text = &source[line_start..line_end];
    line_start + line_text.char_indices().nth(column).map_or(line_text.len(), |(i, _)| i)
}

fn rewrite(source: &str) -> Result<String> {
    let mut file: File = syn::parse_file(source).context("Rustソースの解析に失敗しました")?;
    let mut migrator = Migrator::new();
    migrator.visit_file_mut(&mut file);
    let offsets = line_offsets(source);
    let mut edits: Vec<_> = migrator.replacements.into_iter().map(|(span, text)| {
        let start = span.start();
        let end = span.end();
        (offset(source, &offsets, start.line, start.column), offset(source, &offsets, end.line, end.column), text)
    }).collect();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut output = source.to_owned();
    for (start, end, replacement) in edits {
        output.replace_range(start..end, &replacement);
    }
    Ok(output)
}

fn rust_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() { return Ok((path.extension().is_some_and(|e| e == "rs")).then_some(path.to_owned()).into_iter().collect()); }
    let mut files = Vec::new();
    for entry in fs::read_dir(path).with_context(|| format!("{} の読み込みに失敗しました", path.display()))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() { files.extend(rust_files(&p)?); }
        else if p.extension().is_some_and(|e| e == "rs") { files.push(p); }
    }
    Ok(files)
}

fn process(path: &Path, check: bool) -> Result<bool> {
    let source = fs::read_to_string(path).with_context(|| format!("{} の読み込みに失敗しました", path.display()))?;
    let rewritten = rewrite(&source).with_context(|| path.display().to_string())?;
    if rewritten == source { return Ok(false); }
    println!("{}", path.display());
    if !check { fs::write(path, rewritten).with_context(|| format!("{} の書き込みに失敗しました", path.display()))?; }
    Ok(true)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut changed = false;
    for path in &args.paths {
        for file in rust_files(path)? { changed |= process(&file, args.check)?; }
    }
    if args.check && changed { bail!("未変換の文字列リテラルがあります"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::rewrite;

    #[test]
    fn preserves_comments_and_only_wraps_string_literals() {
        let source = "// keep\nfn main() { let x = \"hello\"; let n = 1; t!(\"done\"); println!(\"log\"); }\n";
        let output = rewrite(source).unwrap();
        assert_eq!(output, "// keep\nfn main() { let x = \"hello\"; let n = 1; t!(\"done\"); println ! (\"{}\" , t ! (\"log\")); }\n");
    }

    #[test]
    fn handles_unicode_before_a_literal() {
        let source = "fn main() { let prefix = \"日本語\"; let text = \"hello\"; }\n";
        assert_eq!(rewrite(source).unwrap(), source);
    }

    #[test]
    fn translates_output_macros_and_format_arguments() {
        let source = "fn main() { eprintln!(\"キューをスキップ: {error}\"); }\n";
        assert_eq!(rewrite(source).unwrap(), "fn main() { eprintln ! (\"{}\" , t ! (\"キューをスキップ: %{arg0}\" , arg0 = format ! (\"{}\" , error))); }\n");
    }

    #[test]
    fn translates_fixed_tr_calls_but_leaves_dynamic_calls() {
        let source = "fn ui(label: &str) { ui.heading(tr(\"NeoUtl - プロジェクト\")); ui.label(tr(label)); }\n";
        assert_eq!(rewrite(source).unwrap(), "fn ui(label: &str) { ui.heading(t ! (\"NeoUtl - プロジェクト\")); ui.label(tr(label)); }\n");
    }
}
