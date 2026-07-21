<p align="center"><img src= "assets/icon-shadowed.svg"/></p>
<h1 align="center">NeoUtl</h1>
<p align="center">
<a href="https://neoutl.taisho-guy.org">公式サイト</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl">Codeberg</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/wiki/Home">Wiki</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/src/branch/aviqtl">AviQtl</a>
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
  - プロプライエタリで、ソースコードも非公開です。
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
  - 古典的なC++言語での実装により、メモリ由来の不正を防ぎきれません。
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

これらの達成により、AviUtlを「仕方なく」使う方々の最適解になることを目指しています。

## 開発状況

開発状況は[NeoUtlのお部屋](https://neoutl.taisho-guy.org)でご覧下さい。

ロードマップは[TODO.md](https://codeberg.org/taisho-guy/NeoUtl/src/branch/main/TODO.md)でご覧下さい。

## ダウンロード方法

[NeoUtlのお部屋](https://neoutl.taisho-guy.org)をご確認下さい。

## ビルド方法

x86_64又はARM64のCPUで動作するLinux/macOS/Windows上でビルド可能です。

### 共通作業

```fish
git clone "https://codeberg.org/taisho-guy/NeoUtl.git"
```

```fish
cd NeoUtl
```
<details>
<summary>NeoUtlの場合</summary>
  
予めRust、Clang、Mold（Linuxの場合）、gstreamerをインストールしてください。

```fish
git switch main
```

```fish
cargo xtask build
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
|動画デコード・エンコード|[gpu-video](https://crates.io/crates/gpu-video) + [symphonia](https://crates.io/crates/symphonia) / [gstreamer](https://gstreamer.freedesktop.org/)|

## 派生

| プロジェクト | 開発者 | 場所 | エンジン | 状況 |
| --- | --- | --- | --- | --- |
| NeoUtl | [taisho-guy](https://codeberg.org/taisho-guy) | [`main`ブランチ](https://codeberg.org/taisho-guy/NeoUtl/src/branch/main) | wgpu | ✅️ 実装中 |
| AviQtl | [taisho-guy](https://codeberg.org/taisho-guy) / [GT-610](https://codeberg.org/GT610) | [`aviqtl`ブランチ](https://codeberg.org/taisho-guy/NeoUtl/src/branch/aviqtl) | Qt Quick | ❌️ 開発終了 |
| AviQtl Plus | [GT-610](https://github.com/GT-610) | [GitHub](https://github.com/GT-610/AviQtl-Plus) | Qt Quick | ✅️ AviQtlのフォーク |

## 貢献方法

理想のアーキテクチャを追求するために、開発初期段階である現在はコントリビュートを受け付けておりません。

0.1.0に達した段階でプルリクエストを含めたコントリビュートをお受けする予定です。

気になる点がございましたらお気軽にイシューをお立て下さい。

## ライセンス

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/agpl.html>.
