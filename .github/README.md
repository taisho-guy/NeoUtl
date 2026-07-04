<h1 align="center">NeoUtl @ <a href="https://codeberg.org/taisho-guy/NeoUtl">Codeberg</a></h1>
<p align="center">
<a href="https://neoutl.taisho-guy.org">NeoUtlのお部屋</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl">Codebergリポジトリ</a> /
<a href="https://github.com/taisho-guy/NeoUtl">GitHubミラー</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/releases">リリース</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/wiki/Home">Wiki</a> /
<a href="https://zenn.dev/taisho_guy/articles/2fa8e91a6c0a07">Zenn</a>
</p>
<p align="center">
    Video editing software aiming to replace and surpass AviUtl. Compatible with Windows, macOS, and Linux.
</p>

## 目的

**AviUtl 1.10**及び**ExEdit 0.92**の操作感を持ちつつ、より安全・高速・柔軟な動画編集ソフトを開発するプロジェクトです。クロスプラットフォームなAviUtlクローンの開発を通じ、AviUtlを「仕方なく」使う方々の最適解になることを目指しております。

## 目標

- [x] 自由ライセンスかつ無料
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

## 採用技術

NeoUtlはRust言語で実装されています。

|項目|採用クレート|
|---|---|
|GUI|[Slint](https://slint.dev/)|
|プレビュー|[wgpu](https://wgpu.rs/)|
|ECS|[Shipyard](https://github.com/leudz/shipyard)|
|非同期処理|[tokio](https://tokio.rs/)|
|動画デコード・エンコード|[ffmpeg-next](https://github.com/zmwangx/rust-ffmpeg)|

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
