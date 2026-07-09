<h1 align="center">NeoUtl</h1>
<p align="center">
<a href="https://neoutl.taisho-guy.org">公式サイト</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl">Codeberg</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/wiki/Home">Wiki</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/src/branch/aviqtl">AviQtl</a> / 
<a href="https://zenn.dev/taisho_guy/articles/2fa8e91a6c0a07">Zenn</a>
</p>
<p align="center">
    Video editing software aiming to replace and surpass AviUtl. Compatible with Windows, macOS, and Linux.
</p>

## 目的

<details>
<summary>AviUtl 1.10/ExEdit 0.92のような操作感</summary>

- NeoUtl
  - AviUtl 1.10とExEdit 0.92（そして数々の拡張）に酷似することを目標としています。
- AviUtl
  - ExEdit2への移行に伴い、新たに独自の操作感を構築しています。
</details>

<details>
<summary>自由ライセンスかつ無料</summary>

- NeoUtl
  - AGPLv3+でライセンスされた自由ソフトウェアで、無料で配布されています。
  - ソースコードも無料で入手できます。
- AviUtl
  - 自由ソフトウェアではなく、ソースコードも非公開です。
</details>

<details>
<summary>Linux / macOS / Windowsに対応</summary>

- NeoUtl
  - Linuxを中心に複数OSをサポートしています。
- AviUtl
  - Windowsでしかネイティブに動作しません。
</details>

<details>
<summary>クラッシュ・フリーズ知らず</summary>

- NeoUtl
  - Rust言語での実装により、メモリ由来の不正を構造的に排除しています。
- AviUtl
  - 古典的なC++言語での実装により、メモリ由来の不正を完全には防ぎきれません。
</details>

<details>
<summary>ハイパフォーマンスかつ超省資源</summary>

- NeoUtl
  - Shyphardを用いたデータ指向設計を実践。CPUのキャッシュ効率を最大化しています。これにより、大量のクリップの処理性能が大幅に向上します。
  - Vulkan、Metal、DirectX12等、各プラットフォームに最適なネイティブAPIを呼び出し、GPUのパフォーマンスを最大限に引き出します。
  - 処理をなるべくGPU内で完結させるゼロコピー設計により、高解像度編集でのボトルネックを排除します。
- AviUtl
  - 古典的なオブジェクト指向設計に基づいており、CPUのキャッシュミスが頻発します。
  - DirectX11のみにしか対応しておりません。
  - エフェクト毎にテクスチャがCPUとGPU間を行き来することが多く、大きなボトルネックが発生します。
</details>

<details>
<summary>柔軟で堅牢なアーキテクチャ</summary>

- NeoUtl
  - モジュール化を徹底し、「疎」で読みやすいソースコードを維持。拡張が容易です。
- AviUtl
  - 外部拡張はSDKに依存。内部のロジックが不透明で、プラグイン作者の「職人技」がエコシステムを左右します。
</details>

詳細なロードマップは[TODO.md](https://codeberg.org/taisho-guy/NeoUtl/src/branch/main/TODO.md)でご確認下さい。

これらの達成により、AviUtlを「仕方なく」使う方々の最適解になることを目指すプロジェクトです。

## 開発状況

まだ開発初期段階ですが、順調です。安定版の一般公開は2027年度以降になりそうです。

## ビルド方法

現在のNeoUtlはビルドを公開しておりません。NeoUtlを試すには適切に環境構築を行い、ビルドする必要があります。

### 共通作業

```fish
git clone "https://codeberg.org/taisho-guy/NeoUtl.git"
```

```fish
cd NeoUtl
```
<details>
<summary>NeoUtlの場合</summary>
  
予めRust、Clang、Mold、FFmpegをインストールしてください。

```fish
git switch main
```

```fish
cargo build
```

実行可能ファイルは`target/debug`あるいは`target/release`以下に生成されます。

</details>

<details>
<summary>AviQtlの場合</summary>

予めPython3、PySide6をインストールしてください。

```fish
git switch aviqtl
```

```fish
python3 BUILD.py
```

実行可能ファイルは`build`以下に生成されます。

</details>

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
