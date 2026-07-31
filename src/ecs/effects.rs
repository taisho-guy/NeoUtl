use shipyard::Component;

use crate::ecs::types::{EffectInstance, EffectParam, Value};

/// エフェクトメタデータ・パラメータスキーマはneoutl-effect-api経由のcdylibプラグインが
/// 保持する（EFFECT_REGISTRY静的配列は廃止）。ホストはcrate::effects::loaderへ委譲する。
pub use neoutl_effect_api::{EffectMeta, ParamKind};
pub type ParamSchema = neoutl_effect_api::EffectParamSchema;

pub fn find_effect(id: &str) -> Option<&'static EffectMeta> {
    crate::effects::loader::by_id(id).map(|p| unsafe { &*((p.vtable.meta)()) })
}

/// EffectMeta.param_schema_ptr/lenから'staticなParamSchemaスライスを得る。
/// # Safety
/// meta.param_schema_ptrはparam_schema_len個の有効なParamSchemaを指し続けること
/// （cdylibプラグインのstatic配列を指すため、プロセス生存中は常に有効）。
pub fn param_schema(meta: &EffectMeta) -> &'static [ParamSchema] {
    if meta.param_schema_ptr.is_null() || meta.param_schema_len == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(meta.param_schema_ptr, meta.param_schema_len) }
}

/// Clipに付随するエフェクトの順序付きスタック。
/// AviQtl概念の「Effect[] (ordered list)」に相当。
#[derive(Clone, Debug, Default, Component)]
pub struct EffectStack(pub Vec<EffectInstance>);

impl EffectStack {
    /// 追加時にスキーマのdefault値をパラメータ初期値として展開する。
    pub fn push(&mut self, effect_id: impl Into<String>) {
        let effect_id = effect_id.into();
        let mut instance = EffectInstance::new(effect_id.clone());
        if let Some(meta) = find_effect(&effect_id) {
            for p in param_schema(meta) {
                let key = unsafe { p.key.as_str() }.to_owned();
                let value = match p.kind {
                    ParamKind::Bool => Value::Bool(p.default_float != 0.0),
                    ParamKind::Enum => Value::Enum(p.default_float as u32),
                    ParamKind::Text => Value::Text(String::new()),
                    ParamKind::FilePath => Value::FilePath(String::new()),
                    ParamKind::Track => Value::TrackRef(-1),
                    ParamKind::Float | ParamKind::Color => Value::Number(p.default_float),
                    ParamKind::Separator | ParamKind::Group => continue,
                    ParamKind::Folder => Value::FilePath(String::new()),
                };
                instance.params.insert(key, EffectParam::new(value));
            }
        }
        self.0.push(instance);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.0.len() {
            self.0.remove(index);
        }
    }

    /// 指定位置へEffectInstanceを挿入する（貼り付け用）。indexが末尾超過の場合は末尾へ追加する。
    pub fn insert(&mut self, index: usize, instance: EffectInstance) {
        let index = index.min(self.0.len());
        self.0.insert(index, instance);
    }

    /// 指定位置のEffectInstanceを直後へ複製する。
    pub fn duplicate(&mut self, index: usize) {
        if let Some(item) = self.0.get(index).cloned() {
            self.0.insert(index + 1, item);
        }
    }

    pub fn set_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(e) = self.0.get_mut(index) {
            e.enabled = enabled;
        }
    }

    pub fn set_param_f32(&mut self, index: usize, key: &str, value: f32) {
        self.set_param_value(index, key, Value::Number(value));
    }

    pub fn set_param_bool(&mut self, index: usize, key: &str, value: bool) {
        self.set_param_value(index, key, Value::Bool(value));
    }

    pub fn set_param_text(&mut self, index: usize, key: &str, value: String) {
        self.set_param_value(index, key, Value::Text(value));
    }

    pub fn set_param_path(&mut self, index: usize, key: &str, value: String) {
        self.set_param_value(index, key, Value::FilePath(value));
    }

    pub fn set_param_enum(&mut self, index: usize, key: &str, value: u32) {
        self.set_param_value(index, key, Value::Enum(value));
    }

    pub fn set_param_track_ref(&mut self, index: usize, key: &str, value: i32) {
        self.set_param_value(index, key, Value::TrackRef(value));
    }

    /// 基準値のみを更新する。既存の中間点は保持する
    /// （挿入で丸ごと置換すると編集のたびに中間点が消える欠陥になるため、
    /// 既存エントリがあればEffectParam::set_staticへ委譲する）。
    pub fn set_param_value(&mut self, index: usize, key: &str, value: Value) {
        if let Some(e) = self.0.get_mut(index) {
            match e.params.get_mut(key) {
                Some(p) => p.set_static(value),
                None => {
                    e.params.insert(key.to_owned(), EffectParam::new(value));
                }
            }
        }
    }

    /// 指定フレームへ中間点を1件設定する。パラメータ未初期化なら
    /// valueを基準値として新規作成する。
    pub fn set_keyframe(
        &mut self,
        index: usize,
        key: &str,
        frame: i32,
        value: f32,
        easing: crate::ecs::types::Easing,
    ) {
        if let Some(e) = self.0.get_mut(index) {
            e.params
                .entry(key.to_owned())
                .or_insert_with(|| EffectParam::new(Value::Number(value)))
                .set_keyframe(frame, value, easing);
        }
    }

    pub fn remove_keyframe(&mut self, index: usize, key: &str, frame: i32) {
        if let Some(e) = self.0.get_mut(index)
            && let Some(p) = e.params.get_mut(key)
        {
            p.remove_keyframe(frame);
        }
    }

    /// 指定エフェクト・パラメータの中間点をold_frameからnew_frameへ移動する。
    /// 対象が存在しない、または移動先に既存点がある場合はfalseを返す。
    pub fn move_keyframe(
        &mut self,
        index: usize,
        key: &str,
        old_frame: i32,
        new_frame: i32,
    ) -> bool {
        match self.0.get_mut(index).and_then(|e| e.params.get_mut(key)) {
            Some(p) => p.move_keyframe(old_frame, new_frame),
            None => false,
        }
    }

    /// split_frame（絶対フレーム）でクリップを分割する。呼び出し元自身は各エフェクト・
    /// 各パラメータの前半のみを残し、返り値が後半用のEffectStack（エフェクト構成・
    /// enabled状態は同一のまま複製、パラメータのみEffectParam::split_atへ委譲）となる。
    pub fn split_at(&mut self, split_frame: i32) -> EffectStack {
        let second: Vec<EffectInstance> = self
            .0
            .iter_mut()
            .map(|e| {
                let mut second_params = std::collections::HashMap::new();
                for (key, param) in e.params.iter_mut() {
                    second_params.insert(key.clone(), param.split_at(split_frame));
                }
                EffectInstance {
                    effect_id: e.effect_id.clone(),
                    enabled: e.enabled,
                    params: second_params,
                }
            })
            .collect();
        EffectStack(second)
    }
}

/// 有効エフェクトのパラメータを「指定フレームで評価した値」で列挙。
/// GPU実行は renderer 側の責務。
pub fn compute_effect_params_at(
    stack: &EffectStack,
    frame: i32,
) -> Vec<(String, std::collections::HashMap<String, Value>)> {
    stack
        .0
        .iter()
        .filter(|e| e.enabled)
        .map(|e| {
            let mut evaluated = std::collections::HashMap::new();
            for (k, p) in &e.params {
                evaluated.insert(k.clone(), p.evaluate(frame));
            }
            (e.effect_id.clone(), evaluated)
        })
        .collect()
}
