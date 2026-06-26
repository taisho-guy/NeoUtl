// src/objects/mod.rs
pub mod cube;
pub mod tetrahedron;
pub mod text;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderKind {
    Tetrahedron = 0,
    Cube = 1,
    /// wgpu_text で描画されるテキストオブジェクト
    Text = 2,
}
