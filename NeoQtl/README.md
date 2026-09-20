# NeoQtl
NeoQtlは、NeoUtlのGUI（egui）をAviQtlのGUI（QML）に置き換えるプロジェクトです。

# ビルド・実行方法
予めQt6、FFmpegをインストールして下さい。
```fish
git clone https://codeberg.org/taisho-guy/NeoUtl.git
cd NeoUtl/NeoQtl
cargo xtask build --release
./target/release/NeoQtl
```