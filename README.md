<p align="center"><img src= "assets/icon-shadowed.svg"/></p>
<h1 align="center">NeoUtl</h1>
<p align="center">
<a href="https://neoutl.taisho-guy.org">公式サイト</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl">Codeberg</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/wiki/Home">Wiki</a> /
<a href="https://codeberg.org/taisho-guy/NeoUtl/src/branch/aviqtl">AviQtl</a>
</p>
<p align="center">
NeoUtl: Ever Optimize &mdash; Until Triumphing Liberty.
</p>

## NeoUtlとは

AviUtl ExEdit0 ライクな動画編集ソフトウェアです。LinuxやWindowsで動作します。macOSも将来的にサポートする予定です。

<img src="assets/screenshot.webp"/>

## 開発状況

開発状況は[NeoUtlのお部屋](https://neoutl.taisho-guy.org)でご覧下さい。

ロードマップは[TODO.md](https://codeberg.org/taisho-guy/NeoUtl/src/branch/main/TODO.md)でご覧下さい。

## ダウンロード方法

[NeoUtlのお部屋](https://neoutl.taisho-guy.org)をご確認下さい。

## ビルド方法

`CONTRIBUTING.md`をご確認下さい。

## 採用技術

NeoUtlはRust言語で実装されています。

|項目|採用クレート|
|---|---|
|GUI|[egui](https://www.egui.rs/)|
|プレビュー|[wgpu](https://wgpu.rs)|
|シェーダ|[Slang](https://shader-slang.org/)|
|ECS|[Shipyard](https://github.com/leudz/shipyard)|
|非同期処理|[tokio](https://tokio.rs/)|
|デコード・エンコード|[FFmpeg](https://ffmpeg.org/)|

## 派生

| プロジェクト | 開発者 | 場所 | エンジン | 状況 |
| --- | --- | --- | --- | --- |
| NeoUtl | [taisho-guy](https://codeberg.org/taisho-guy) | [`main`ブランチ](https://codeberg.org/taisho-guy/NeoUtl/src/branch/main) | wgpu | ✅️ 実装中 |
| AviQtl | [taisho-guy](https://codeberg.org/taisho-guy) / [GT-610](https://codeberg.org/GT610) | [`aviqtl`ブランチ](https://codeberg.org/taisho-guy/NeoUtl/src/branch/aviqtl) | Qt Quick | ❌️ 開発終了 |
| AviQtl Plus | [GT-610](https://github.com/GT-610) | [GitHub](https://github.com/GT-610/AviQtl-Plus) | Qt Quick | ✅️ AviQtlのフォーク |

## 貢献方法

プルリクエストについては[貢献の初め方](https://codeberg.org/taisho-guy/NeoUtl/issues/53)をご覧下さい。

バグ報告、提案、議論などについては、[イシュー](https://codeberg.org/taisho-guy/NeoUtl/issues)を作成して下さい。

プルリクエスト、イシュー共に、テンプレートに従って下さい。日本語でお願い致します。

## ライセンス

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/agpl.html>.
