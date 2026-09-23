use neoutl_shared_abi::StrRef;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[repr(C)]
pub struct ExpressionHostVTable {
    pub get_property:
        unsafe extern "C" fn(object_id: usize, prop_name: StrRef, fallback: f32) -> f32,
    pub get_time_seconds: unsafe extern "C" fn() -> f64,
    pub get_frame: unsafe extern "C" fn() -> i32,
    pub get_fps: unsafe extern "C" fn() -> f32,
    pub get_object_layer: unsafe extern "C" fn(object_id: usize) -> i32,
}
unsafe impl Send for ExpressionHostVTable {}
unsafe impl Sync for ExpressionHostVTable {}

#[repr(C)]
pub struct ExpressionEngineMeta {
    pub id: StrRef,
    pub name: StrRef,
    pub version: StrRef,
}
unsafe impl Send for ExpressionEngineMeta {}
unsafe impl Sync for ExpressionEngineMeta {}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ExpressionEvalContext {
    pub object_id: usize,
    pub frame: i32,
    pub time_seconds: f64,
    pub current_value: f32,
}

#[repr(C)]
pub struct ExpressionEngineVTable {
    pub meta: unsafe extern "C" fn() -> *const ExpressionEngineMeta,
    pub bind_host: unsafe extern "C" fn(host: *const ExpressionHostVTable),
    pub compile: unsafe extern "C" fn(script: StrRef) -> u64,
    pub evaluate: unsafe extern "C" fn(handle: u64, ctx: *const ExpressionEvalContext) -> f32,
    pub release: unsafe extern "C" fn(handle: u64),
}
unsafe impl Send for ExpressionEngineVTable {}
unsafe impl Sync for ExpressionEngineVTable {}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_expression_engine_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const ExpressionEngineVTable;

pub fn bind_expression_host(engine: &ExpressionEngineVTable, host: *const ExpressionHostVTable) {
    unsafe {
        (engine.bind_host)(host);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Number(f32),
    Variable(String),
    PropRef(String),
    Func(String),
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    LParen,
    RParen,
    Comma,
}

#[derive(Clone, Debug)]
pub struct CompiledExpression {
    pub rpn: Vec<Token>,
}

fn tokenize(expr: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c.is_ascii_digit() || (c == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let start = i;
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let num: f32 = s
                .parse()
                .map_err(|e| format!("Invalid number '{}': {}", s, e))?;
            tokens.push(Token::Number(num));
            continue;
        }

        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();

            if ident == "prop" {
                while i < len && chars[i].is_whitespace() {
                    i += 1;
                }
                if i < len && chars[i] == '(' {
                    i += 1;
                    while i < len && chars[i].is_whitespace() {
                        i += 1;
                    }
                    if i < len && (chars[i] == '\'' || chars[i] == '"') {
                        let quote = chars[i];
                        i += 1;
                        let p_start = i;
                        while i < len && chars[i] != quote {
                            i += 1;
                        }
                        let prop_name: String = chars[p_start..i].iter().collect();
                        if i < len && chars[i] == quote {
                            i += 1;
                        }
                        while i < len && chars[i].is_whitespace() {
                            i += 1;
                        }
                        if i < len && chars[i] == ')' {
                            i += 1;
                        }
                        tokens.push(Token::PropRef(prop_name));
                        continue;
                    }
                }
            }

            // 関数判定
            let mut peek = i;
            while peek < len && chars[peek].is_whitespace() {
                peek += 1;
            }
            if peek < len && chars[peek] == '(' {
                tokens.push(Token::Func(ident));
            } else {
                tokens.push(Token::Variable(ident));
            }
            continue;
        }

        match c {
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Multiply),
            '/' => tokens.push(Token::Divide),
            '%' => tokens.push(Token::Modulo),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            ',' => tokens.push(Token::Comma),
            _ => return Err(format!("Unexpected character: '{}'", c)),
        }
        i += 1;
    }

    Ok(tokens)
}

fn precedence(token: &Token) -> i32 {
    match token {
        Token::Plus | Token::Minus => 1,
        Token::Multiply | Token::Divide | Token::Modulo => 2,
        _ => 0,
    }
}

// Shunting-yard アルゴリズムで中置記法から逆ポーランド記法 (RPN) へ変換
fn to_rpn(tokens: &[Token]) -> Result<Vec<Token>, String> {
    let mut output = Vec::new();
    let mut op_stack = Vec::new();

    let mut prev_is_op = true;

    for token in tokens {
        match token {
            Token::Number(_) | Token::Variable(_) | Token::PropRef(_) => {
                output.push(token.clone());
                prev_is_op = false;
            }
            Token::Func(_) => {
                op_stack.push(token.clone());
                prev_is_op = true;
            }
            Token::Comma => {
                while let Some(top) = op_stack.last() {
                    if *top == Token::LParen {
                        break;
                    }
                    output.push(op_stack.pop().unwrap());
                }
                prev_is_op = true;
            }
            Token::Plus | Token::Minus | Token::Multiply | Token::Divide | Token::Modulo => {
                // 単項マイナスの処理 (0 - x)
                if *token == Token::Minus && prev_is_op {
                    output.push(Token::Number(0.0));
                }

                let p = precedence(token);
                while let Some(top) = op_stack.last() {
                    if *top == Token::LParen {
                        break;
                    }
                    if precedence(top) >= p {
                        output.push(op_stack.pop().unwrap());
                    } else {
                        break;
                    }
                }
                op_stack.push(token.clone());
                prev_is_op = true;
            }
            Token::LParen => {
                op_stack.push(Token::LParen);
                prev_is_op = true;
            }
            Token::RParen => {
                let mut found_lparen = false;
                while let Some(top) = op_stack.pop() {
                    if top == Token::LParen {
                        found_lparen = true;
                        break;
                    }
                    output.push(top);
                }
                if !found_lparen {
                    return Err("Mismatched parentheses".to_string());
                }
                if let Some(Token::Func(_)) = op_stack.last() {
                    output.push(op_stack.pop().unwrap());
                }
                prev_is_op = false;
            }
        }
    }

    while let Some(top) = op_stack.pop() {
        if top == Token::LParen || top == Token::RParen {
            return Err("Mismatched parentheses".to_string());
        }
        output.push(top);
    }

    Ok(output)
}

impl CompiledExpression {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let tokens = tokenize(expr)?;
        let rpn = to_rpn(&tokens)?;
        Ok(Self { rpn })
    }

    pub fn evaluate(
        &self,
        ctx: &ExpressionEvalContext,
        host: Option<&ExpressionHostVTable>,
    ) -> f32 {
        let mut stack: Vec<f32> = Vec::new();

        for token in &self.rpn {
            match token {
                Token::Number(val) => stack.push(*val),
                Token::Variable(var) => match var.as_str() {
                    "time" | "t" => stack.push(ctx.time_seconds as f32),
                    "frame" | "f" => stack.push(ctx.frame as f32),
                    "value" | "val" | "v" => stack.push(ctx.current_value),
                    "fps" => {
                        let fps = if let Some(h) = host {
                            unsafe { (h.get_fps)() }
                        } else {
                            30.0
                        };
                        stack.push(fps);
                    }
                    "pi" | "PI" => stack.push(std::f32::consts::PI),
                    _ => {
                        // ホストコールバック経由でプロパティとして解決試行
                        let val = if let Some(h) = host {
                            let str_ref = StrRef {
                                ptr: var.as_ptr(),
                                len: var.len(),
                            };
                            unsafe { (h.get_property)(ctx.object_id, str_ref, 0.0) }
                        } else {
                            0.0
                        };
                        stack.push(val);
                    }
                },
                Token::PropRef(prop_name) => {
                    let val = if let Some(h) = host {
                        let str_ref = StrRef {
                            ptr: prop_name.as_ptr(),
                            len: prop_name.len(),
                        };
                        unsafe { (h.get_property)(ctx.object_id, str_ref, 0.0) }
                    } else {
                        0.0
                    };
                    stack.push(val);
                }
                Token::Plus => {
                    let b = stack.pop().unwrap_or(0.0);
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a + b);
                }
                Token::Minus => {
                    let b = stack.pop().unwrap_or(0.0);
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a - b);
                }
                Token::Multiply => {
                    let b = stack.pop().unwrap_or(1.0);
                    let a = stack.pop().unwrap_or(1.0);
                    stack.push(a * b);
                }
                Token::Divide => {
                    let b = stack.pop().unwrap_or(1.0);
                    let a = stack.pop().unwrap_or(0.0);
                    if b.abs() <= 1e-7 {
                        stack.push(0.0);
                    } else {
                        stack.push(a / b);
                    }
                }
                Token::Modulo => {
                    let b = stack.pop().unwrap_or(1.0);
                    let a = stack.pop().unwrap_or(0.0);
                    if b.abs() <= 1e-7 {
                        stack.push(0.0);
                    } else {
                        stack.push(a % b);
                    }
                }
                Token::Func(name) => match name.to_lowercase().as_str() {
                    "sin" => {
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.sin());
                    }
                    "cos" => {
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.cos());
                    }
                    "tan" => {
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.tan());
                    }
                    "abs" => {
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.abs());
                    }
                    "sqrt" => {
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.max(0.0).sqrt());
                    }
                    "floor" => {
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.floor());
                    }
                    "ceil" => {
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.ceil());
                    }
                    "min" => {
                        let b = stack.pop().unwrap_or(0.0);
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.min(b));
                    }
                    "max" => {
                        let b = stack.pop().unwrap_or(0.0);
                        let a = stack.pop().unwrap_or(0.0);
                        stack.push(a.max(b));
                    }
                    "clamp" => {
                        let max_v = stack.pop().unwrap_or(1.0);
                        let min_v = stack.pop().unwrap_or(0.0);
                        let val = stack.pop().unwrap_or(0.0);
                        stack.push(val.clamp(min_v, max_v));
                    }
                    _ => {}
                },
                Token::LParen | Token::RParen | Token::Comma => {}
            }
        }

        stack.pop().unwrap_or(ctx.current_value)
    }
}

// ---------------------------------------------------------------------------
// グローバルな標準Expressionエンジンインスタンス & VTable
// ---------------------------------------------------------------------------

struct GlobalEngineState {
    host: Option<*const ExpressionHostVTable>,
    compiled: HashMap<u64, CompiledExpression>,
}
unsafe impl Send for GlobalEngineState {}
unsafe impl Sync for GlobalEngineState {}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static GLOBAL_STATE: OnceLock<Mutex<GlobalEngineState>> = OnceLock::new();

fn global_state() -> &'static Mutex<GlobalEngineState> {
    GLOBAL_STATE.get_or_init(|| {
        Mutex::new(GlobalEngineState {
            host: None,
            compiled: HashMap::new(),
        })
    })
}

static ENGINE_META: ExpressionEngineMeta = ExpressionEngineMeta {
    id: StrRef::from_str("neoutl.expression.standard"),
    name: StrRef::from_str("NeoUtl Standard Math Expression Engine"),
    version: StrRef::from_str("1.0.0"),
};

unsafe extern "C" fn std_meta() -> *const ExpressionEngineMeta {
    &ENGINE_META
}

unsafe extern "C" fn std_bind_host(host: *const ExpressionHostVTable) {
    let mut state = global_state().lock().unwrap();
    state.host = Some(host);
}

unsafe extern "C" fn std_compile(script: StrRef) -> u64 {
    let script_str = unsafe { script.as_str() };
    match CompiledExpression::parse(script_str) {
        Ok(compiled) => {
            let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
            let mut state = global_state().lock().unwrap();
            state.compiled.insert(handle, compiled);
            handle
        }
        Err(err) => {
            eprintln!("[Expression] Compile error: {}", err);
            0
        }
    }
}

unsafe extern "C" fn std_evaluate(handle: u64, ctx: *const ExpressionEvalContext) -> f32 {
    let context = if ctx.is_null() {
        ExpressionEvalContext {
            object_id: 0,
            frame: 0,
            time_seconds: 0.0,
            current_value: 0.0,
        }
    } else {
        unsafe { *ctx }
    };

    let state = global_state().lock().unwrap();
    if let Some(expr) = state.compiled.get(&handle) {
        let host_ref = state.host.and_then(|p| unsafe { p.as_ref() });
        expr.evaluate(&context, host_ref)
    } else {
        context.current_value
    }
}

unsafe extern "C" fn std_release(handle: u64) {
    let mut state = global_state().lock().unwrap();
    state.compiled.remove(&handle);
}

pub static STANDARD_EXPRESSION_ENGINE_VTABLE: ExpressionEngineVTable = ExpressionEngineVTable {
    meta: std_meta,
    bind_host: std_bind_host,
    compile: std_compile,
    evaluate: std_evaluate,
    release: std_release,
};

#[unsafe(no_mangle)]
pub extern "C" fn neoutl_expression_engine_entry() -> *const ExpressionEngineVTable {
    &STANDARD_EXPRESSION_ENGINE_VTABLE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expression_parser_and_evaluation() {
        let expr = CompiledExpression::parse("time * 10 + 5").unwrap();
        let ctx = ExpressionEvalContext {
            object_id: 1,
            frame: 30,
            time_seconds: 2.5,
            current_value: 0.0,
        };
        let val = expr.evaluate(&ctx, None);
        assert!((val - 30.0).abs() < 1e-5); // 2.5 * 10 + 5 = 30.0

        let expr2 = CompiledExpression::parse("sin(time) * 100").unwrap();
        let val2 = expr2.evaluate(&ctx, None);
        assert!((val2 - (2.5f32.sin() * 100.0)).abs() < 1e-5);

        let expr3 = CompiledExpression::parse("clamp(value + 10, 0, 50)").unwrap();
        let ctx3 = ExpressionEvalContext {
            object_id: 1,
            frame: 0,
            time_seconds: 0.0,
            current_value: 45.0,
        };
        let val3 = expr3.evaluate(&ctx3, None);
        assert_eq!(val3, 50.0); // 45 + 10 = 55 -> clamped to 50
    }

    #[test]
    fn test_vtable_and_host_binding() {
        static DUMMY_HOST: ExpressionHostVTable = ExpressionHostVTable {
            get_property: dummy_get_property,
            get_time_seconds: dummy_get_time,
            get_frame: dummy_get_frame,
            get_fps: dummy_get_fps,
            get_object_layer: dummy_get_layer,
        };

        unsafe extern "C" fn dummy_get_property(
            _obj_id: usize,
            prop_name: StrRef,
            _fallback: f32,
        ) -> f32 {
            let name = unsafe { prop_name.as_str() };
            if name == "X" { 123.4 } else { 0.0 }
        }

        unsafe extern "C" fn dummy_get_time() -> f64 {
            3.0
        }
        unsafe extern "C" fn dummy_get_frame() -> i32 {
            90
        }
        unsafe extern "C" fn dummy_get_fps() -> f32 {
            30.0
        }
        unsafe extern "C" fn dummy_get_layer(_obj_id: usize) -> i32 {
            0
        }

        bind_expression_host(&STANDARD_EXPRESSION_ENGINE_VTABLE, &DUMMY_HOST);

        let script = StrRef::from_str("prop('X') * 2 + time");
        let handle = unsafe { (STANDARD_EXPRESSION_ENGINE_VTABLE.compile)(script) };
        assert_ne!(handle, 0);

        let ctx = ExpressionEvalContext {
            object_id: 42,
            frame: 90,
            time_seconds: 3.0,
            current_value: 0.0,
        };

        let res = unsafe { (STANDARD_EXPRESSION_ENGINE_VTABLE.evaluate)(handle, &ctx) };
        // 123.4 * 2 + 3.0 = 246.8 + 3.0 = 249.8
        assert!((res - 249.8).abs() < 1e-4);

        unsafe {
            (STANDARD_EXPRESSION_ENGINE_VTABLE.release)(handle);
        }
    }
}
