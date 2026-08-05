# i18n-literal-migrator

`syn` で Rust の AST を解析し、標準出力マクロ（`print!`・`println!`・`eprint!`・`eprintln!`）と、固定文字列を受け取る `tr("...")` / `effect_name("...")` などを `t!()` に変換します。通常の文字列リテラル、動的な `tr(label)`、環境変数名、パス、設定キーは変更しません。書き戻しは対象ノードの span のみを置換します。

```sh
cargo run --manifest-path tools/i18n-literal-migrator/Cargo.toml -- src crates
cargo run --manifest-path tools/i18n-literal-migrator/Cargo.toml -- --check src
```

なお `t!` や `println!` のマクロ本体は `syn` 上で opaque な token stream なので、その内部の文字列は訪問対象になりません。`println!` 以外のマクロ引数も同様に安全のため変更しません。
