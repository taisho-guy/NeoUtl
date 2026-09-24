# NeoUtl Plugin SDK (C / C++) - v0.7.5

C および C++ から NeoUtl の全プラグイン（エフェクト、オブジェクト、イージング、数式エンジン、拡張機能）を開発するための公式 SDK です。
C99 標準規格に準拠しており、GCC、Clang、MSVC 等の一般的な C/C++ コンパイラで追加ライブラリなしに共有ライブラリ（`.so` / `.dll`）をビルドできます。

---

## 1. ヘッダー構成 (`include/neoutl/`)

- `neoutl.h`: すべてのヘッダーを一度に読み込むマスターヘッダー
- `neoutl_abi.h`: 共通基本型（文字列参照 `neoutl_str_ref_t`、スライス `neoutl_ffi_slice_t`、パラメータ定義、ROI、ハードウェアアクセラレータ構造体）
- `neoutl_effect.h`: エフェクトプラグイン用 ABI（`neoutl_effect_entry`、WGSL シェーダー連携、ユニフォームパック関数）
- `neoutl_object.h`: カスタムオブジェクト用 ABI（`neoutl_object_entry`、幾何頂点、レンダーコンテキスト）
- `neoutl_easing.h`: イージング補間エンジン用 ABI（`neoutl_easing_engine_entry`、キーフレーム補間）
- `neoutl_expression.h`: 数式評価エンジン用 ABI（`neoutl_expression_engine_entry`）
- `neoutl_extension.h`: 拡張プラグインおよび AviUtl2 互換構造体（`HOST_APP_TABLE`, `EDIT_HANDLE`, `COMMON_PLUGIN_TABLE` 等）

---

## 2. コンパイル方法

### Linux (GCC / Clang)

```bash
# エフェクトプラグインのビルド例
gcc -O2 -Wall -Wextra -fPIC -Iinclude -shared -o libsample_effect.so examples/sample_effect.c

# オブジェクトプラグインのビルド例
gcc -O2 -Wall -Wextra -fPIC -Iinclude -shared -o libsample_object.so examples/sample_object.c
```

### Windows (MSVC)

```cmd
cl /O2 /LD /Iinclude examples\sample_effect.c /Fe:sample_effect.dll
```

### CMake を使用する場合

```bash
cd examples
mkdir build && cd build
cmake ..
cmake --build .
```

---

## 3. NeoUtl へのプラグイン配置

ビルドされた共有ライブラリ（`.so` または `.dll`）を NeoUtl の各フォルダに配置します：

- **エフェクトプラグイン**: `NeoUtl/effects/`
- **オブジェクトプラグイン**: `NeoUtl/objects/`
- **拡張プラグイン (魔改造 / Common)**: `NeoUtl/extensions/`
- **イージングプラグイン**: `NeoUtl/plugins/easing/`
