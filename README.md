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

## アーキテクチャ

```mermaid
flowchart TD
    subgraph UI["1. ユーザーインターフェース層 (egui / winit)"]
        UI_Core["UIエンジン / メインループ (src/ui, src/egui_loop.rs)"]
        Timeline["タイムライン UI (src/ui/timeline/)"]
        Preview["プレビューウィンドウ (src/ui/preview.rs)"]
        Dialogs["ダイアログ & エディタ (src/ui/dialogs.rs, project_settings.rs)"]
        ThemeLoc["テーマ & 多言語化 (src/theme.rs, src/localization.rs)"]
    end

    subgraph DataState["2. データモデル & 状態管理層"]
        Document["ドキュメントモデル (Undo/Redo スナップショット) (src/document.rs)"]
        ECS["Shipyard ECS ワールド (src/ecs/)"]
        Schema["Protobuf スキーマ (neoutl-schema, src/schema.rs)"]
        Project["プロジェクトマネージャー (src/project.rs)"]
    end

    subgraph CoreServices["3. コアサブシステム"]
        subgraph MediaSubsystem["メディアサブシステム (tokio)"]
            MediaRuntime["メディアランタイム & ワーカー (src/media/)"]
            MediaDecoders["デコーダー群 (neo-media-ffmpeg, symphonia, image)"]
            MediaCache["メディアキャッシュ & 波形 (neo-media-cache, waveform.rs)"]
        end

        subgraph AudioSubsystem["オーディオサブシステム"]
            AudioMixer["オーディオミキサー (src/audio/mixer.rs)"]
            AudioHost["Maolan ホストアダプター (maolan-host-adapter)"]
            AudioPlayback["Rodio 再生エンジン (rodio)"]
        end

        subgraph RenderEngine["描画エンジン (wgpu)"]
            RenderPipeline["描画パイプライン & エフェクトチェーン (src/renderer/)"]
            SlangShaders["Slang シェーダービルド (neoutl-*-shader-build)"]
            GPUShared["GPU 共有リソース (src/gpu_shared.rs)"]
        end
    end

    subgraph ExtensionLayer["4. プラグイン & 拡張エコシステム"]
        ObjLoader["オブジェクトローダー & API (src/objects/loader.rs, neoutl-object-api)"]
        FxLoader["エフェクトローダー & API (src/effects/loader.rs, neoutl-effect-api)"]
        LuaRuntime["Lua エンジン (neoutl-lua-runtime, neoutl-effect-lua)"]
        EasingAPI["イージングエンジン (neoutl-easing-api, neoutl-easing-standard)"]
        HotReload["ホットリロードマネージャー (src/hot_reload.rs)"]
    end

    subgraph ExportSubsystem["5. エクスポートサブシステム"]
        ExportEngine["エクスポートパイプライン (src/export.rs)"]
    end

    %% データフロー接続
    UI_Core -->|ユーザー操作| Document
    Document -->|スナップショット変換 / 同期| ECS
    Project -->|シリアライズ / デシリアライズ| Schema
    Schema -->|読み込み / 保存| Document

    ECS -->|コンポーネント照会 & 座標変換| RenderPipeline
    ECS -->|音声ストリームパラメータ| AudioMixer
    ECS -->|フレーム / 波形の取得要求| MediaRuntime

    MediaRuntime -->|パケットのデコード| MediaDecoders
    MediaDecoders -->|フレームテクスチャ / キャッシュ| MediaCache
    MediaCache -->|テクスチャの転送| GPUShared

    AudioMixer -->|ホストプラグインの処理| AudioHost
    AudioMixer -->|音声のストリーミング再生| AudioPlayback

    RenderPipeline -->|Slangシェーダーの実行| SlangShaders
    RenderPipeline -->|フレーム描画| Preview

    ObjLoader -->|オブジェクトの登録| ECS
    FxLoader -->|エフェクトチェーンの適用| RenderPipeline
    LuaRuntime -->|スクリプトの評価実行| FxLoader
    EasingAPI -->|補間カーブの計算| ECS
    HotReload -->|cdylibプラグインの再読み込み| ObjLoader
    HotReload -->|cdylibプラグインの再読み込み| FxLoader

    ExportEngine -->|ECS状態の読み込み| ECS
    ExportEngine -->|フレームのレンダリング| RenderPipeline
    ExportEngine -->|音声のミキシング| AudioMixer
    ExportEngine -->|出力データのエンコード| MediaDecoders
```

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
