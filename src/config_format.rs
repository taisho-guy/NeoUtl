// src/config_format.rs
use std::collections::HashMap;

/// フラットな `key = value` 形式ファイルを解析し、キーと値文字列の対応表を返す。
/// 値は生テキストのまま返却する（文字列値は引用符付き、真偽値・数値は無加工）。
pub fn parse_kv(source: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(mut parser) = tree_sitter_language_pack::get_parser("toml") else {
        return map;
    };
    let Some(tree) = parser.parse(source) else {
        return map;
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();
    let text_of = |node: &tree_sitter_language_pack::Node| -> String {
        let r = node.byte_range();
        String::from_utf8_lossy(&bytes[r.start..r.end])
            .trim()
            .to_string()
    };

    for i in 0..root.child_count() {
        let Some(node) = root.child(i as u32) else {
            continue;
        };
        if node.kind() != "pair" {
            continue;
        }
        let key_node = node.child_by_field_name("key");
        let value_node = node.child_by_field_name("value");
        if let (Some(k), Some(v)) = (key_node, value_node) {
            map.insert(text_of(&k), text_of(&v));
        }
    }
    map
}

/// `key = value` 形式の行を連結してファイル内容を生成する。
pub fn format_kv(pairs: &[(&str, String)]) -> String {
    let mut out = String::new();
    for (key, value) in pairs {
        out.push_str(key);
        out.push_str(" = ");
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// TOML文字列値の前後引用符を除去し、エスケープを復元する。
pub fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"'));
    match inner {
        Some(body) => body.replace("\\\"", "\"").replace("\\\\", "\\"),
        None => trimmed.to_string(),
    }
}

/// 任意文字列をTOML文字列値として引用符で囲む（内部に引用符・バックスラッシュを含めない前提の単純用途）。
pub fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

pub fn get_bool(map: &HashMap<String, String>, key: &str, fallback: bool) -> bool {
    map.get(key).map(|v| v == "true").unwrap_or(fallback)
}

pub fn get_int(map: &HashMap<String, String>, key: &str, fallback: i32) -> i32 {
    map.get(key)
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(fallback)
}

pub fn get_u32(map: &HashMap<String, String>, key: &str, fallback: u32) -> u32 {
    map.get(key)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(fallback)
}

pub fn get_string(map: &HashMap<String, String>, key: &str, fallback: &str) -> String {
    map.get(key)
        .map(|v| unquote(v))
        .unwrap_or_else(|| fallback.to_string())
}
