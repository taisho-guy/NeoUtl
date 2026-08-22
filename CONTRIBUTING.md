# 環境構築

## 依存関係のインストール

<details><summary>Linuxの場合</summary>
ClangとRust、Git、LuaJIT、MakeFile、Carla、FFmpegをインストールしてください。

最新のArch Linux系ディストリビューションでのビルドを推奨します。
</details>

<details><summary>Windowsの場合</summary>
MSVCとRust、Git、LuaJIT、vcpkg、CMakeをインストールしてください。

Windows11以降でのビルドを推奨します。

Visual Studio付属のvcpkgは使用できません（クラシックモードを用いる必要がある為）。
</details>

<details><summary>macOSの場合</summary>
持ってないのでわかりません。

ビルドを検証してくださる方を募集しております。
</details>

## リポジトリのクローン

```fish
git clone "https://codeberg.org/taisho-guy/NeoUtl.git"
git submodule update --init --recursive
```

<details><summary>Windowsの場合は追加でこちらを実行します</summary>

```fish
vcpkg install ffmpeg:x64-windows --clean-after-build
```

</details>

## ビルド

```fish
cargo xtask build --release
```

万が一ビルドが通らない場合は、イシューにてご相談ください。

# 開発のルール

## 中途半端な状態のプルリクエストは[WIP]にする

マージできる品質になるまで、プルリクエスト名の先頭に[WIP]を追加してください。

未完成の状態でもプルリクエストを作成いただいて構いません。

## コメントを使わない

勿論、[WIP]状態のときは、ご自身の為に、コメントを用いることが可能です。

但し、完成された、マージを待つコードにコメントは含まれてはいけません。

コメントに依存せず、ソースコード自体がわかりやすくなるようにしてください。

ワークスペースのルートで

```fish
luajit clean.lua
```

を実行することで、コメントをすべて削除することができます。

## フォーマット

コミットの前に

```fish
cargo fmt --all
```

する習慣を付けてください。

# イシュー・プルリクエスト

テンプレートが用意されています。テンプレートに従ってください。

# その他

[貢献の始め方](https://codeberg.org/taisho-guy/NeoUtl/issues/53)を参照してください。