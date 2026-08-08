fn main() {
    neoutl_object_shader_build::compile_compute_spirv(
        "semi_planar_to_rgba",
        "slang/semi_planar_to_rgba.slang",
        "cs_main",
    );
}
