//! Expression engine plugin API for property math, scripting, and reactive bindings.

pub use neoutl_expression_api::{
    CompiledExpression, ENTRY_SYMBOL, EntryFn, ExpressionEngineMeta, ExpressionEngineVTable,
    ExpressionEvalContext, ExpressionHostVTable, Token, bind_expression_host,
};
