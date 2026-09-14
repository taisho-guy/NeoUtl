use neoutl_expression_api::{CompiledExpression, STANDARD_EXPRESSION_ENGINE_VTABLE};
use neoutl_shared_abi::StrRef;
use shipyard::Component;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionBinding {
    pub script: String,
    pub enabled: bool,
    pub compiled_handle: u64,
}

#[derive(Clone, Debug, Default, Component)]
pub struct PropertyExpressions {
    pub bindings: HashMap<String, ExpressionBinding>,
}

#[allow(dead_code)]
impl PropertyExpressions {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn set_expression(&mut self, prop_key: &str, script: &str, enabled: bool) -> bool {
        let script_str = script.trim();
        if script_str.is_empty() {
            self.remove_expression(prop_key);
            return false;
        }

        if CompiledExpression::parse(script_str).is_err() {
            return false;
        }

        let str_ref = StrRef {
            ptr: script_str.as_ptr(),
            len: script_str.len(),
        };
        let handle = unsafe { (STANDARD_EXPRESSION_ENGINE_VTABLE.compile)(str_ref) };
        if handle == 0 {
            return false;
        }

        if let Some(old) = self.bindings.get(prop_key) {
            if old.compiled_handle != 0 {
                unsafe {
                    (STANDARD_EXPRESSION_ENGINE_VTABLE.release)(old.compiled_handle);
                }
            }
        }

        self.bindings.insert(
            prop_key.to_string(),
            ExpressionBinding {
                script: script_str.to_string(),
                enabled,
                compiled_handle: handle,
            },
        );

        true
    }

    pub fn get_expression(&self, prop_key: &str) -> Option<(&str, bool)> {
        self.bindings
            .get(prop_key)
            .map(|b| (b.script.as_str(), b.enabled))
    }

    pub fn remove_expression(&mut self, prop_key: &str) -> bool {
        if let Some(binding) = self.bindings.remove(prop_key) {
            if binding.compiled_handle != 0 {
                unsafe {
                    (STANDARD_EXPRESSION_ENGINE_VTABLE.release)(binding.compiled_handle);
                }
            }
            true
        } else {
            false
        }
    }
}
