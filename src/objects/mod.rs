// src/objects/mod.rs
pub mod cube;
pub mod tetrahedron;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderKind {
    Tetrahedron = 0,
    Cube = 1,
}
