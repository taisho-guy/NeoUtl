# NeoQtl
NeoQtlは、NeoUtlのGUI（egui）をAviQtlのGUI（QML）に置き換えるプロジェクトです。

## ビルド・実行方法
予めQt6、FFmpegをインストールして下さい。
```fish
git clone https://codeberg.org/taisho-guy/NeoUtl.git
cd NeoUtl/NeoQtl
cargo xtask build --release
./target/release/NeoQtl
```

## 現在の開発状況
まだプロトタイプ段階で、NeoUtlやAviQtl（更に言えばAviUtl）程の完成度はありません。