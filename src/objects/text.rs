// src/objects/text.rs
//
// テキストオブジェクトは wgpu_text クレートで描画されます。
// ジオメトリ（頂点バッファ等）は不要なため、このモジュールはマーカーとしてのみ機能します。
//
// テキスト描画の流れ:
//   1. ECS: TextContent コンポーネントが文字列・位置・サイズ・色を保持
//   2. System: get_active_objects_system() が ActiveObject を収集
//   3. Renderer: TextBrush::queue() でセクションを登録し draw() で描画
