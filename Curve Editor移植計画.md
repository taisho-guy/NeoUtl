# AviUtl `curve_editor` → NeoUtl 移植計画

対象元: `curve-editor.xml`（AviUtl拡張編集プラグイン、C++/WebView2構成）
対象先: `project_context_20260806_120104.xml` の以下3クレート
- `neoutl-easing-api`（FFI ABI定義）
- `neoutl-easing-standard`（標準エンジン実装）
- `src/ui/properties/easing_editor.rs`（egui編集UI）

方針: `egui_plot 0.36`を描画基盤に採用。中間点(キーフレーム)がパラメーター単位で独立管理される現行NeoUtl仕様は維持。それ以外（カーブ種別、モディファイア、プリセット、操作系、UI構成）はAviUtl側の設計に1:1で合わせる。

---

## 1. 現状データモデルの問題点

`neoutl-easing-standard::StandardEasing`は以下1個のフラットenumに全機能を圧縮している。

```rust
enum StandardEasing {
    Linear, Step,
    EaseInSine, EaseOutSine, EaseInOutSine,   // ...Penner系24種
    Bezier { cp1: (f32,f32), cp2: (f32,f32) },
    Random { seed: u32, step: i32 },
}
```

問題点は4点。

1. **カーブ種別とモディファイアが同一階層に混在**。`Random`はAviUtl側では独立した`NoiseModifier`（既存カーブへの後掛けレイヤー）であり、単独のカーブ種別ではない。
2. **物理カーブが名前近似のPenner関数で代替**。`EaseOutBounce`等は減衰率・反発係数を持たず固定式。AviUtl `BounceCurve`は反発係数(COR)・周期を持つ可変パラメータ物理モデル。`Elastic`系カーブはNeoUtl側に不在。
3. **セグメント内マルチポイント曲線が不在**。AviUtl `NormalCurve`はセグメントをさらに複数の子セグメントへ細分化できるが、NeoUtlは1区間=1カーブ固定。
4. **モディファイア非対応・プリセット非対応・カーブID参照非対応**。スタック合成・再利用・保存済みカーブ呼び出しの3機能が欠落。

---

## 2. AviUtl側データモデル（抽出結果）

### 2.1 クラス階層

```
Curve (抽象, id: u32, locked: bool)
└─ GraphCurve (抽象, anchor_start/end: Point<f64>, prev/next: *GraphCurve, modifiers: Vec<Modifier>)
   ├─ LinearCurve                          区間内直線
   ├─ BezierCurve : NumericGraphCurve       3次ベジェ、handle_left/right(角度固定・長さ固定・対称移動)
   ├─ BounceCurve : NumericGraphCurve       cor(反発係数), period, reversed
   ├─ ElasticCurve : NumericGraphCurve      amplitude, frequency, decay, reversed
   ├─ NormalCurve                           curve_segments_: Vec<Box<GraphCurve>>（子セグメント配列、add/delete/replace可）
   ├─ ValueCurve                            curve_segments_: Vec<Box<GraphCurve>>（数値軸表示版NormalCurve）
   └─ ScriptCurve                           Lua式評価

Modifier (抽象, name, enabled, curve: *GraphCurve)
├─ DiscretizationModifier   sampling_resolution, quantization_resolution（出力を離散量子化）
├─ NoiseModifier            seed, amplitude, frequency, phase, octaves, decay_sharpness（FastNoiseLite）
├─ SineWaveModifier         amplitude, frequency, phase
└─ SquareWaveModifier       amplitude, frequency, phase, duty比

GraphCurveEditor (グローバルレジストリ)
├─ curves_normal_: Vec<NormalCurve>   idx_normal_でカーブID切替（前後移動・末尾ジャンプ・削除・複製・コピペ）
├─ curves_value_:  Vec<ValueCurve>    同上（数値軸版）
└─ curve_bezier_/curve_elastic_/curve_bounce_: 単一の「作業台」インスタンス（セグメント種別変更時に複製元として使用）
```

### 2.2 評価則

- `Curve::get_value(progress, start, end)` = `Modifier`チェーンを外側から`apply()`で合成した関数に`curve_function(progress, start, end)`を通した結果。
- `Modifier::apply(CurveFunction) -> CurveFunction`は関数デコレータであり、1セグメントに複数個スタック可能（順序が意味を持つ）。
- `NormalCurve::curve_function`は`progress`が属する子セグメントを二分探索し、そのセグメントのローカル`progress`へ再写像して委譲する（再帰構造）。

### 2.3 UI構成（WebView2 + React/TSX、ロジックはTypeScriptへ既に純粋分離済み）

- `panel_main.tsx`: カーブ一覧パネル（サムネイル`curve_thumbnail.tsx`、プリセット`preset.tsx`、ドラッグ&ドロップ並べ替え）
- `panel_editor.tsx`: グラフ本体（`editor_graph.tsx`）+ 数値入力欄（`editor_text.tsx`）の2ペイン
- `curve_grid.tsx`: 座標軸・グリッド描画
- `controls/control_*.ts`: 各カーブ種別ごとのハンドル当たり判定・ドラッグ変換（正規化座標⇔画面座標）
- `context_menu.cpp` / `dialog_*.cpp`: 右クリックメニュー、数値ダイアログ（制御点座標直接入力）、IDジャンプダイアログ、プリセット管理ダイアログ

---

## 3. 移植後のNeoUtlデータモデル設計

### 3.1 crate構成の変更

`neoutl-easing-api`は現行FFI ABI（`KeyframeC`/`EasingEngineVTable`）を維持。第三者エンジンのプラグイン差し替え余地を潰さない。変更は`neoutl-easing-standard`の内部表現のみに限定する。

```rust
// crates/easings/neoutl-easing-standard/src/curve.rs（新設）

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CurveKind {
    Linear,
    Bezier   { handle_left: [f32; 2], handle_right: [f32; 2] },
    Bounce   { cor: f32, period: f32, reversed: bool },
    Elastic  { amplitude: f32, frequency: f32, decay: f32, reversed: bool },
    Normal   { segments: Vec<CurveSegment> },   // セグメント内細分化
    Script   { source: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurveSegment {
    pub anchor_start: [f32; 2],   // 正規化座標(0..1, 任意実数可)
    pub anchor_end:   [f32; 2],
    pub kind: CurveKind,
    pub modifiers: Vec<Modifier>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Modifier {
    Discretization { sampling_resolution: u32, quantization_resolution: u32 },
    Noise          { seed: i32, amplitude: f32, frequency: f32, phase: f32, octaves: i32, decay_sharpness: f32 },
    SineWave       { amplitude: f32, frequency: f32, phase: f32 },
    SquareWave     { amplitude: f32, frequency: f32, phase: f32, duty: f32 },
}
```

`Modifier::apply`はAviUtlの関数デコレータ方式をそのままRustのクロージャ合成へ移す。

```rust
pub fn build_evaluator(kind: &CurveKind) -> impl Fn(f32, f32, f32) -> f32 + '_ { /* curve_function相当 */ }

pub fn apply_modifiers(base: impl Fn(f32) -> f32, mods: &[Modifier]) -> impl Fn(f32) -> f32 {
    mods.iter().fold(Box::new(base) as Box<dyn Fn(f32) -> f32>, |f, m| m.wrap(f))
}
```

### 3.2 セグメントとNeoUtlキーフレームの結線

現行`Keyframe{ frame, value, engine_id, engine_payload }`は維持。`engine_payload`のJSON内容を`StandardEasing`から`CurveKind`へ置換する（`parse_payload`/`encode_payload`のシリアライズ対象型のみ変更、呼び出し側APIシグネチャは不変）。1キーフレーム=1セグメントの起点という対応関係も不変。これにより「パラメーター単位で独立した中間点列を持つ」現行仕様を保ったまま、区間ごとのカーブ表現をAviUtl相当まで拡張する。

### 3.3 カーブID・プリセットレジストリ

```rust
// src/easings/registry.rs（新設）
pub struct CurveRegistry {
    named: Vec<(String, CurveKind)>,   // プリセット（保存名, カーブ本体）
}
```

AviUtlの`curves_normal_`（連番IDでの参照・切替）は、NeoUtlでは「名前付きプリセット」として実装する。理由: NeoUtlの中間点はパラメーター単位で独立しているため、AviUtl的な「行の位置=ID」という暗黙対応が成立しない。番号ではなく名前でのプリセット参照に置き換える（機能的等価、UI体験は「プリセット一覧から選択」に統一）。

---

## 4. UI設計（`src/ui/properties/easing_editor.rs`）

2ペイン構成に再編する。現行は単一`Plot`+`Grid`の縦積みレイアウト。

```
┌─────────────────────────────┬───────────────────────────┐
│ 左ペイン: セグメント/プリセット一覧 │ 右ペイン: グラフ編集        │
│ - キーフレーム行（frame/value）    │ - egui_plot::Plot          │
│ - 選択中セグメントのカーブ種別     │ - ハンドルドラッグ           │
│   コンボ（Linear/Bezier/Bounce/  │ - グリッド線                │
│   Elastic/Normal/Script）        │ - 数値ダイアログ(右クリック) │
│ - モディファイアスタック          │                             │
│   (+追加 / ✕削除 / ↑↓並替)      │                             │
│ - プリセット適用/保存ボタン        │                             │
└─────────────────────────────┴───────────────────────────┘
```

### 4.1 `egui_plot`によるハンドル操作

現行実装の`Bezier`制御点ドラッグ（`u.pointer_coordinate()` + `dist2`最近傍判定）を全カーブ種別へ一般化する。

```rust
enum HandleId { AnchorStart, AnchorEnd, BezierLeft, BezierRight, BounceParam, ElasticAmp, ElasticFreqDecay }

fn hit_test(pointer: PlotPoint, handles: &[(HandleId, [f32;2])]) -> Option<HandleId> {
    handles.iter().min_by(|a, b| dist2(pointer, a.1).total_cmp(&dist2(pointer, b.1)))
        .filter(|(_, p)| dist2(pointer, *p) < HIT_RADIUS_SQ)
        .map(|(id, _)| *id)
}
```

修飾キー対応（AviUtl `BezierHandle`の角度固定/長さ固定/対称移動）は`egui::Modifiers`で代替。

| AviUtl操作 | キー | egui実装 |
|---|---|---|
| ハンドル角度固定 | Shift押下 | `ui.input(|i| i.modifiers.shift)`でスナップ角(15°刻み)適用 |
| ハンドル長さ固定 | Ctrl押下 | ドラッグ中は角度のみ更新、長さは`lock_length()`相当で凍結 |
| 対称移動 | Alt押下 | 反対側ハンドルへ点対称座標を同時書込み |

### 4.2 数値ダイアログ

AviUtl `dialog_control_position.cpp`相当。egui標準の`egui::Window`によるモーダル代替で機能等価（別ネイティブウィンドウ化は不要、既存`WindowKind::EasingEditor`内で完結）。制御点を右クリックで開き、`DragValue`直接入力にフォーカスする。

### 4.3 モディファイアUI

セグメント選択時、モディファイアスタックをリスト表示。各行は種別コンボ+パラメータ+有効/無効チェック+削除ボタン。グラフ上のプレビュー曲線は`apply_modifiers`済みの最終波形をサンプリング（現行`SAMPLES=48`点サンプリングを維持、モディファイア込みでも同一ロジックで動作）。

---

## 5. 段階的実装計画

### フェーズ0（前提）: Cargo依存修正
`egui_plot = "0.36"`へ変更（済）。追加依存なし。

### フェーズ1: データモデル置換
1. `crates/easings/neoutl-easing-standard/src/curve.rs`新設、`CurveKind`/`CurveSegment`/`Modifier`定義。
2. `Linear`/`Bezier`/`Bounce`/`Elastic`の`curve_function`をAviUtl `curve_bezier.cpp`/`curve_bounce.cpp`/`curve_elastic.cpp`の数式へ置換（現行Penner近似式は破棄）。
3. `parse_payload`/`encode_payload`の対象型を`StandardEasing`→`CurveKind`へ変更。旧`.nprj`互換のため`StandardEasing`→`CurveKind`のワンショット変換関数を用意し読込時のみ適用（書込は新形式固定）。
4. `evaluate_c`（FFI境界）内の区間探索ロジックは変更不要（区間探索はKeyframeレベルであり型置換の影響を受けない）。

### フェーズ2: Normalカーブ（セグメント細分化）
1. `CurveKind::Normal { segments }`の`curve_function`実装（progress二分探索→ローカル再写像、AviUtl `NormalCurve::curve_function`のロジックを移植）。
2. セグメント追加/削除/種別変更API（`add_segment`/`remove_segment`/`replace_segment`）を`CurveSegment`操作として実装。

### フェーズ3: モディファイア
1. `Modifier::wrap(Box<dyn Fn(f32)->f32>) -> Box<dyn Fn(f32)->f32>`をDiscretization/SineWave/SquareWaveの3種で実装（数式はAviUtl該当`.cpp`から移植、除算・剰余のみで構成され外部クレート不要）。
2. `Noise`は`noise = "0.9"`を`crates/easings/neoutl-easing-standard/Cargo.toml`へ追加し`noise::Fbm<Perlin>`で`octaves`合成、`seed`は`Perlin::new(seed as u32)`へ直結。

### フェーズ3.5: Scriptカーブ
1. `CurveKind::Script { source }`の評価関数を`neoutl-lua-runtime`経由で実装。評価毎に使い捨て`Lua`インスタンスを生成し、`t`/`start`/`end`の3グローバルのみ注入、`io`/`os`/`require`テーブルは未登録のまま（デフォルト非公開）とする。
2. 命令数上限フックを追加し、超過時は`fallback`値（区間開始値）を返す。
3. UI側は`egui::TextEdit::multiline`でソース編集、変更確定時のみ再評価（毎フレーム評価は行わない）。

### フェーズ4: UI再構成
1. `easing_editor.rs`を2ペインレイアウトへ書き換え。
2. ハンドル種別ごとの`hit_test`/ドラッグ処理を一般化関数へ集約。
3. モディファイアスタックUIを追加。

### フェーズ5: プリセット・レジストリ
1. `CurveRegistry`実装、保存先は`src/easings/loader.rs`が読む設定ディレクトリ配下へJSON追記。
2. プリセット一覧UI（左ペイン下部）、適用/現在値から新規保存ボタン。

### フェーズ6: 検証
1. 旧`.nprj`（`StandardEasing`形式）を読み込み、変換後カーブが視覚的に旧描画と一致することを`SAMPLES`点比較で確認。
2. 主要5カーブ種別（Linear/Bezier/Bounce/Elastic/Normal）のグラフ形状をAviUtl側`curve_*.cpp`出力とオフラインで数値比較。

---

## 6. 決定事項

1. **Noiseモディファイア**: `noise`クレート導入。`noise::Perlin`を1次元入力(`x = frame軸正規化値 * frequency + phase`)でサンプリングし、`amplitude`・`octaves`（`noise::Fbm<Perlin>`でオクターブ合成）・`decay_sharpness`（出力振幅への冪乗減衰）をAviUtl `NoiseModifier`のパラメータ名のまま踏襲する。`seed`は`Perlin::new(seed as u32)`へ直結。
2. **Scriptカーブ**: 採用。`neoutl-lua-runtime`を利用しつつ最低限のサンドボックス化を行う。範囲は以下3点に限定する。
   - 実行時間上限: 1評価あたり命令数上限を設定（`mlua`系であれば`set_hook`で命令カウンタ監視、上限到達で強制中断）。
   - I/O遮断: `io`/`os`/`require`等の標準ライブラリテーブルを評価用`Lua`インスタンスから除去し、公開グローバルは`t`（0..1正規化progress）・`start`・`end`の3変数と算術/三角関数相当の許可関数のみに限定。
   - 副作用禁止: グローバル変数書込みを評価毎に破棄（サンドボックス用`Lua`インスタンスをセグメント評価ごとに使い捨てるか、評価前にグローバルテーブルをスナップショット復元）。
3. **旧`StandardEasing`ペイロード互換**: 変換関数（`StandardEasing`→`CurveKind`）は現行メジャーバージョン(`0.5.x`)の間のみ保持する。次回メジャーバージョンで削除し、以降は旧形式`.nprj`読込不可（読込失敗時はLinearへフォールバックし警告ログを出す）とする。

---

## 7. 影響範囲まとめ

| ファイル | 変更内容 |
|---|---|
| `crates/easings/neoutl-easing-standard/src/lib.rs` | `StandardEasing`定義を`curve.rs`新設ファイルへ分離・置換 |
| `crates/easings/neoutl-easing-standard/src/curve.rs` | 新設。`CurveKind`/`CurveSegment`/`Modifier`/評価関数 |
| `crates/neoutl-easing-api/src/lib.rs` | 変更不要（FFI ABI境界は不変） |
| `src/easings/loader.rs` | プリセット読込処理追加 |
| `src/ui/properties/easing_editor.rs` | 2ペイン化、ハンドル操作一般化、モディファイアUI追加 |
| `Cargo.toml`（ワークスペース） | `egui_plot = "0.36"`（済） |
| `crates/easings/neoutl-easing-standard/Cargo.toml` | `noise = "0.9"`追加、`neoutl-lua-runtime`依存追加 |
