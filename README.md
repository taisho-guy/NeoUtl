<p align="center">
  <img src="./assets/splash.svg" width="256">
</p>

<p align="center"><b>AviUtlを踏襲し凌駕する次世代動画編集ソフト</b></p>

> [!IMPORTANT]
> AviQtlは現在大規模リファクタリング中です。
> - プレビューをQt Quick 3DからFilamentに移行し、データフローを最適化します。
> - 新機能の追加やバグ修正、ドキュメントの整備が遅れます。

## AviQtlとは

**AviUtl 1.10** & **ExEdit 0.92**の操作感を踏襲しつつ、**AviUtlを超える性能**を持つ動画編集ソフトを開発するプロジェクトです。
GPUを使った**高速で強力なエフェクト**や、VST3やLV2等の**音声エフェクト**をかけることもできます。
**Linux**、**Windows**、**macOS**上でビルド・実行できるプラットフォーム非依存設計を採用しています。

**詳細は[AviQtlのお部屋](https://aviqtl.taisho-guy.org)をご確認ください。**

## Q & A

<details>
<summary>開発のきっかけは？</summary>

### OSの壁と決定的敗北
愛用する**CachyOS**上でAviUtlを運用しようとして失敗したことがきっかけです。**AviUtlのためだけにWindows環境を維持し続けること**は受け入れがたいものでした。

### 肥大化したエコシステム
理由は違えど、AviUtlを「仕方なく」使い続けている方は少なくないはずです。長年の拡張によって肥大化した「ハウルの動く城」のようなエコシステムは、不満を抱えながらも手放すことができない存在となっています。

### プロジェクトの目標とミッション
[鹿児島県立甲南高等学校](https://edunet002.synapse-blog.jp/konan/)の課題研究において、この現状を打破すべくAviQtlの独自開発を決意しました。

- **個人的な目標:** Domino、VocalShifter、REAPER、AviUtlをはしごすることなく、Linux上のAviQtlのみで音MADを制作すること。
- **AviQtlのミッション:** AviUtlを「仕方なく」使っている方々の最適解になること。
</details>

<details>
<summary>なぜAviUtlのクローンを開発しているのですか？</summary>

AviQtlは「AviUtlの再発明」ではありません。AviUtlを強く意識していますが、その中身は全く異なります。

| 項目 | AviQtl | AviUtl 1.10 | ExEdit 0.92 |
| :--- | :--- | :--- | :--- |
| 基盤技術 | Qt Quick / Vulkan | Win32 API | Win32 API |
| 並列処理モデル | データ駆動型（ECS） | シングルスレッド | マルチスレッド |
| メモリ空間 | 64bit | 32bit (最大4GB) | 64bit |
| プレビュー描画 | Vulkan | GDI | Direct3D |
| 音声エンジン | Carla (VST3/LV2等) | 標準機能のみ | 標準機能のみ |
| プラグイン方式 | LuaJIT / C++ / QML / GLSL | Lua / C++ | LuaJIT / C++ |
| 対応OS | Linux, Windows, macOS等 | Windows | Windows |

AviQtlは構造的な弱点を根本的に解決します：
1. **ECS（Entity Component System）によるデータ指向:** CPUキャッシュ効率を極限まで高め、大量のオブジェクト処理を高速化。
2. **近代的なメモリ管理:** C++23のスマートポインタを採用し、原因不明のクラッシュを構造的に最小化。
3. **UIとレンダリングの分離:** 重い描画中でもタイムライン操作が妨げられず、High-DPI環境でもUIが鮮明に表示されます。
</details>

<details>
<summary>名称やアイコンの由来は？</summary>

名称は「AviUtl」と「Qt」を組み合わせた造語です。
アイコンは、QtとAviUtlのロゴを組み合わせたデザインになっています。

<p align="center">
  <img src="./assets/qt.svg" width="64" align="middle"> + <img src="./assets/aviutl.svg" width="64" align="middle"> = <img src="./assets/icon.svg" width="64" align="middle">
</p>
</details>

<details>
<summary>WindowsやmacOSでも動きますか？</summary>
LinuxとmacOSに対応しております。Windowsビルドの問題については[イシュー](https://codeberg.org/taisho-guy/AviQtl/issues/2)をご確認下さい。
</details>

<details>
<summary>AviUtlのプラグインは使えますか？</summary>
いいえ。仕組みが異なるため互換性は有りません。
</details>

## ダウンロード

> [!WARNING]
> - 現在AviQtlはLinux(x86_64)、macOS(ARM64)、Windows(x86_64)をターゲットに開発されています。
> - 最新のパッケージが必要なため、Ubuntu等の保守的なディストリビューションでは動作しない可能性があります。
> - **[CachyOS](https://cachyos.org/)**でのご利用を強く推奨致します。

### インストール手順
1. Linuxの場合、以下の依存関係をインストールします：
   - Qt6全般、LuaJIT、Vulkan実装（Mesa等）、FFmpeg、Carla、libc++
2. [リリースページ](https://codeberg.org/taisho-guy/AviQtl/releases)からお使いのPCに最適なビルドをダウンロードします。
3. ファイルを展開し、`AviQtl` に実行権限を付与して実行します。

## ビルド手順

Linuxユーザーであれば`BUILD.py`1つで簡単にビルドできます。

- 依存関係をインストールします

  - Pacman: `sudo pacman -S --needed distrobox podman python pyside6 git`
  - APT: `sudo apt install distrobox podman python3 python3-pyside6 git`
  - DNF: `sudo dnf install distrobox podman python3 python3-pyside6 git`
  - MSYS2: `pacman -S git mingw-w64-ucrt-x86_64-pyside6`
  - Homebrew: `brew install python pyside git`

- リポジトリをクローンします

  - `git clone https://codeberg.org/taisho-guy/AviQtl.git`

- ビルドします

  - `cd AviQtl`
  - `python BUILD.py`

- 実行します

  - Linux/macOS: `./build/AviQtl`
  - MSYS2: `./build/AviQtl.exe`

> [!IMPORTANT]
> - Windowsビルドには[問題](https://codeberg.org/taisho-guy/AviQtl/issues/2)がございます。

## 関連リンク

AviQtlは、多くの素晴らしいプロジェクトの上に成り立っています。

| プロジェクト | ライセンス | 役割 |
| :--- | :--- | :--- |
| AviUtl | 非自由 | リスペクト元 |
| Carla | GPLv2+ | 音声エフェクト（VST3/LV2等）のホスト |
| FFmpeg | GPLv2+ | 動画・音声のデコード / エンコード |
| LuaJIT | MIT | 高速なスクリプトエンジン |
| Qt | GPLv3 | UI/UXフレームワーク |
| Vulkan | Apache 2.0 | GPU描画 / エフェクト基盤 |
| Zrythm | AGPLv3 | 音声プラグイン実装の参考 |
| Remix Icon | Remix Icon License | シンボルアイコン |

## フィードバック・バグの報告

[イシューを作成](https://codeberg.org/taisho-guy/AviQtl/issues/new)して下さい。

些細なことでも、開発に大きく役立ちます。
 
## 貢献

開発への参加を歓迎します！バグ報告、機能提案、コードの寄稿については CONTRIBUTING.md をご覧ください。

> [!NOTE]
> 本プロジェクトへ貢献を送信した場合、あなたは自分の貢献物を AGPL の下で提供することに同意したものとみなします。

## ライセンス

AviQtlは[GNU Affero General Public License](https://www.gnu.org/licenses/agpl-3.0.txt)に基づいて公開されています。

AviQtl内で使用されている[Remix Icon](https://remixicon.com/)は[Remix Icon License](https://raw.githubusercontent.com/Remix-Design/RemixIcon/refs/heads/master/License)に基づいて提供されています。