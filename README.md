# NeoUtl

## 概要

NeoUtlはAviUtlを踏襲し凌駕する動画編集ソフトを目指すプロジェクトです。

### 目標

- [ ] **Linux** / **macOS** / Windowsに対応
- [ ] GPUを用いた**高速なプレビュー**
- [ ] ECSによる**効率的な処理**
- [ ] MV制作に耐えうる**DAW機能**の搭載
- [ ] **AviUtl 1.10** や **ExEdit 0.92**のような操作感

## インストール方法

現在ビルドを提供しておりません。以下の手順でビルドすることが可能です。

## ビルド方法

### リポジトリをクローンします。

```fish
git clone "https://codeberg.org/taisho-guy/NeoUtl"
```

### `NeoUtl`ディレクトリに移動します。
```fish
cd NeoUtl
```

### `dev`ブランチに移動します。

```fish
git switch dev
```

### ビルド・実行します。

```fish
cargo run
```

## `dev`ブランチで行われていること

RustベースのNeoUtlの開発を進めています。`main`はC++ベースのNeoUtlです。Rustでの検証が完了したら、`main`を`dev`の内容で置き換える予定です。

## ライセンス

NeoUtlはGNU Affero General Public License Version 3 or latorでライセンスされており、これに基づいて提供されます。

画像ファイル（PNG、SVG、ICO等）はCC0でライセンスされています。
