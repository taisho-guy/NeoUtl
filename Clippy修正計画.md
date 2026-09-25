## 計画

### 1. 現状固定
- `cargo check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- Clippy診断をカテゴリ別・ファイル別に保存
- 現在の1,379件を基準値として記録

### 2. 低リスク診断
優先順位:

1. `long_literal`
2. `useless_format`
3. `redundant_closure`
4. `collapsible_if`
5. `map().unwrap_or()`
6. `redundant_guard`
7. `wildcard_import`
8. ドキュメント形式
9. 不要な`self`
10. `clone_from`などの効率改善

各カテゴリごとに修正し、毎回以下を実行します。

```text
cargo fmt
cargo check
cargo test
cargo clippy
```

### 3. 境界安全性
次に以下を根本修正します。

- `indexing_slicing`
  - 固定長配列は`get`、スライスは`get`または範囲検証
  - 不変条件がある箇所は専用ヘルパーで検証
  - `unwrap`への置換はしない
- `arithmetic_side_effects`
  - `checked_*`
  - `saturating_*`
  - `wrapping_*`
  - 範囲検証後の計算
  - ドメイン上の単位変換を専用関数化

### 4. 数値変換
変換ごとに方針を決めます。

- 符号付き整数 → 符号なし整数: `try_from`とエラー処理
- 浮動小数点 → 整数: `clamp`後に`round`/`floor`し、明示的に範囲保証
- 整数 → 浮動小数点: `From`が使える場合は置換し、精度低下が仕様上許容される箇所は変換境界を明示
- フレーム番号・解像度・レイヤー番号などは専用の安全変換関数へ集約

### 5. 大規模関数の分割
特に以下を分割します。

- `active_query.rs`
- `ecs/track.rs`
- `ui/properties/easing_editor/mod.rs`
- `project/mod.rs`
- `audio/mixer.rs`

関数単位を小さくし、Clippy診断を局所化します。

### 6. テスト追加
安全修正の影響が大きい箇所にテストを追加します。

- フレーム境界
- 空配列・短い配列
- 負値・過大値の変換
- オーバーフロー候補
- カメラ・プレビュー操作
- メディアキャッシュの容量境界

### 7. 最終確認
抑制属性を追加せず、以下をすべて通します。

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

実装は、まず低リスク診断を一括で減らし、その後に`indexing_slicing`、数値変換、`arithmetic_side_effects`を機能単位で進めるのが最も効率的です。