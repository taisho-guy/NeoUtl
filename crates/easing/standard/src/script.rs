use crate::curve::fbm_noise1;
use mlua::{Lua, StdLib};
use std::sync::atomic::{AtomicU32, Ordering};

const INSTRUCTION_LIMIT: u32 = 100_000;

pub fn evaluate(source: &str, t: f32) -> Option<f32> {
    let lua = Lua::new_with(StdLib::MATH, mlua::LuaOptions::new()).ok()?;

    let count = AtomicU32::new(0);
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(1000),
        move |_, _| {
            let n = count.fetch_add(1, Ordering::Relaxed) + 1;
            if n * 1000 > INSTRUCTION_LIMIT {
                Err(mlua::Error::RuntimeError(
                    "instruction limit exceeded".to_owned(),
                ))
            } else {
                Ok(mlua::VmState::Continue)
            }
        },
    )
    .ok()?;

    let globals = lua.globals();
    globals.set("t", t).ok()?;
    globals.set("st", 0.0f32).ok()?;
    globals.set("ed", 1.0f32).ok()?;

    let noise_fn = lua
        .create_function(
            move |_, (amp, freq, phase, octaves, seed): (f32, f32, f32, i32, i32)| {
                let n = fbm_noise1(seed, t * freq + phase, octaves.max(1) as u32, 1.0);
                Ok(n * amp)
            },
        )
        .ok()?;
    globals.set("noise", noise_fn).ok()?;

    let result: mlua::Result<f32> = lua.load(source).set_name("curve_script").eval();
    result.ok()
}
