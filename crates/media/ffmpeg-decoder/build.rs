fn main() {
    neoutl_object_shader_build::compile_compute_spirv(
        "nv12_to_rgba",
        "slang/nv12_to_rgba.slang",
        "cs_main",
    );
}
