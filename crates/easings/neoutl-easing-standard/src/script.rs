//! `CurveKind::Script`評価。`Curve_Editor移植計画.md` フェーズ3.5、決定事項2対応。
//! 評価毎に使い捨て`Lua`インスタンスを生成し、`t`/`start`/`end`の3グローバルと
//! `math`テーブルのみを公開する。`io`/`os`/`require`は未登録のため到達不能。
//! 命令数上限フックにより無限ループを強制中断する。

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
    );

    let globals = lua.globals();
    globals.set("t", t).ok()?;
    globals.set("start", 0.0f32).ok()?;
    globals.set("end", 1.0f32).ok()?;

    let result: mlua::Result<f32> = lua.load(source).set_name("curve_script").eval();
    result.ok()
}
