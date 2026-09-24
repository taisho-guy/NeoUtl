use crate::curve::fbm_noise1;
use mlua::{Lua, StdLib};
use std::sync::atomic::{AtomicU32, Ordering};

const INSTRUCTION_LIMIT: u32 = 100_000;

pub fn evaluate(source: &str, t: f32) -> Option<f32> {
    evaluate_checked(source, t).ok()
}

pub fn evaluate_checked(source: &str, t: f32) -> Result<f32, String> {
    let lua = Lua::new_with(StdLib::MATH, mlua::LuaOptions::new()).map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

    let globals = lua.globals();
    globals.set("t", t).map_err(|e| e.to_string())?;
    globals.set("st", 0.0f32).map_err(|e| e.to_string())?;
    globals.set("ed", 1.0f32).map_err(|e| e.to_string())?;

    let noise_fn = lua
        .create_function(
            move |_, (amp, freq, phase, octaves, seed): (f32, f32, f32, i32, i32)| {
                let n = fbm_noise1(seed, t * freq + phase, octaves.max(1) as u32, 1.0);
                Ok(n * amp)
            },
        )
        .map_err(|e| e.to_string())?;
    globals.set("noise", noise_fn).map_err(|e| e.to_string())?;

    lua.load(source)
        .set_name("curve_script")
        .eval::<f32>()
        .map_err(|e| e.to_string())
}
