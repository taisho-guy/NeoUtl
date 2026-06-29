# NeoUtl
AviUtlを踏襲し凌駕する次世代動画編集ソフト

## 目的

**AviUtl 1.10**及び**ExEdit 0.92**の面影を残しつつ、より安全・高速・柔軟な動画編集ソフトを開発するプロジェクトです。クロスプラットフォームなAviUtlクローンの開発を通じ、AviUtlを「仕方なく」使う方々の最適解になることを目指しております。

## 目標

- [ ] 自由ライセンスかつ無料
- [ ] **Linux** / **macOS** / Windowsに対応
- [ ] GPUを用いた**高速なプレビュー**
- [ ] ECSによる**効率的な処理**
- [ ] MV制作に耐えうる**DAW機能**の搭載
- [ ] **AviUtl 1.10** や **ExEdit 0.92**のような操作感

## インストール方法

NeoUtlは現在ビルドをリリースしておりません。NeoUtlの源流である [AviQtl](https://codeberg.org/taisho-guy/NeoUtl/releases/tag/0.0.95-Unstable) や [AviQtl Plus](https://github.com/GT-610/NeoUtl-Plus/releases) をお試し下さい。

## ビルド方法


```fish
git clone "https://codeberg.org/taisho-guy/NeoUtl.git"
```

```fish
cd NeoUtl
```

### NeoUtlの場合
  
予めRust、Clang、Mold、FFmpegをインストールしてください。

```fish
git switch main
```

```fish
cargo build
```

実行可能ファイルは`target/debug`あるいは`target/release`以下に生成されます。

### AviQtlの場合

予めPython3、PySide6をインストールしてください。

```fish
git switch aviqtl
```

```fish
python3 BUILD.py
```

実行可能ファイルは`build`以下に生成されます。

## リンク

### [NeoUtlのお部屋](https://neoutl.taisho-guy.org)

公式サイト。エンドユーザー向けの情報を提供しています。

### [Codebergリポジトリ](https://codeberg.org/taisho-guy/NeoUtl)

公式リポジトリ。ソースコードや開発者向けの情報を提供しています。

### [GitHubミラー](https://github.com/taisho-guy/NeoUtl)

Codebergと同様のソースコードをホスト。

### [リリース](https://codeberg.org/taisho-guy/NeoUtl/releases)

Linux / macOS / WIndows向けの実行可能ファイルを提供しています。

### [Wiki](https://codeberg.org/taisho-guy/NeoUtl/wiki/Home)

公式Wiki。各項目における詳細な情報を提供しています。

## 派生

| プロジェクト | 開発者 | 場所 | エンジン | 状況 |
| --- | --- | --- | --- | --- |
| NeoUtl | [taisho-guy](https://codeberg.org/taisho-guy) | [`main`ブランチ](https://codeberg.org/taisho-guy/NeoUtl/src/branch/dev) | wgpu | ✅️ 実装中 |
| AviQtl | [taisho-guy](https://codeberg.org/taisho-guy) / [GT-610](https://codeberg.org/GT610) | [`aviqtl`ブランチ](https://codeberg.org/taisho-guy/NeoUtl/src/branch/aviqtl) | Qt Quick | ❌️ 開発終了 |
| AviQtl Plus | [GT-610](https://github.com/GT-610) | [GitHub](https://github.com/GT-610/AviQtl-Plus) | Qt Quick | ✅️ AviQtlのフォーク |

## ライセンス

画像ファイルは [CC0](https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt) に基づいて提供されます。

ソースコード及びSDKは [GNU Affero General Public License Version 3](https://www.gnu.org/licenses/agpl-3.0.txt) or later に基づいて提供されます。

[Remix Icon](https://remixicon.com/) は [Remix Icon License](https://raw.githubusercontent.com/Remix-Design/RemixIcon/refs/heads/master/License) に基づいて提供されます。これは**自由ライセンスではありません**。

その他のライブラリのライセンスは上記のライセンスと異なる場合がございます。
