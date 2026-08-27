use prost::Message;

pub trait SchemaContract: Sized {
    type Schema;

    fn to_schema(&self) -> Self::Schema;
    fn from_schema(schema: &Self::Schema) -> Result<Self, String>;
}

pub fn encode_schema<T: SchemaContract>(value: &T) -> Vec<u8>
where
    T::Schema: Message,
{
    value.to_schema().encode_to_vec()
}

pub fn decode_schema<T: SchemaContract>(bytes: &[u8]) -> Result<T, String>
where
    T::Schema: Message + Default,
{
    let schema = T::Schema::decode(bytes).map_err(|e| e.to_string())?;
    T::from_schema(&schema)
}

impl SchemaContract for crate::document::DocumentModel {
    type Schema = neoutl_schema::DocumentModel;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::DocumentModel::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::transform::Transform {
    type Schema = neoutl_schema::Transform;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::Transform::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::components::GroupControl {
    type Schema = neoutl_schema::GroupControl;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::GroupControl::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::components::ClipTarget {
    type Schema = neoutl_schema::ClipTarget;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::ClipTarget::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::components::AudioParams {
    type Schema = neoutl_schema::AudioParams;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::AudioParams::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::components::TextContent {
    type Schema = neoutl_schema::TextContent;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::TextContent::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::types::Keyframe {
    type Schema = neoutl_schema::Keyframe;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::Keyframe::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::types::Value {
    type Schema = neoutl_schema::Value;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::Value::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::types::EffectParam {
    type Schema = neoutl_schema::EffectParam;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::EffectParam::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::types::ApplyMode {
    type Schema = i32;

    fn to_schema(&self) -> Self::Schema {
        match self {
            crate::ecs::types::ApplyMode::Linear => neoutl_schema::ApplyMode::Linear as i32,
            crate::ecs::types::ApplyMode::Interpolate => {
                neoutl_schema::ApplyMode::Interpolate as i32
            }
        }
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        match neoutl_schema::ApplyMode::try_from(*schema)
            .map_err(|_| "invalid apply mode".to_string())?
        {
            neoutl_schema::ApplyMode::Linear => Ok(crate::ecs::types::ApplyMode::Linear),
            neoutl_schema::ApplyMode::Interpolate => Ok(crate::ecs::types::ApplyMode::Interpolate),
        }
    }
}

impl SchemaContract for crate::ecs::audio_plugins::PluginInstanceRef {
    type Schema = neoutl_schema::PluginInstanceRef;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::PluginInstanceRef::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::resources::SystemSettingsResource {
    type Schema = neoutl_schema::SystemSettings;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::SystemSettings::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::resources::AudioPluginSettingsResource {
    type Schema = neoutl_schema::AudioPluginSettings;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::AudioPluginSettings::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::ecs::resources::SceneMeta {
    type Schema = neoutl_schema::SceneMeta;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::SceneMeta::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::export::ExportPreset {
    type Schema = neoutl_schema::ExportPreset;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::ExportPreset::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema.clone())
    }
}

impl SchemaContract for crate::shortcuts::KeymapResource {
    type Schema = neoutl_schema::KeymapResource;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::KeymapResource::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

impl SchemaContract for crate::shortcuts::Override {
    type Schema = neoutl_schema::Override;

    fn to_schema(&self) -> Self::Schema {
        neoutl_schema::Override::from(self)
    }

    fn from_schema(schema: &Self::Schema) -> Result<Self, String> {
        Self::try_from(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::SchemaContract;
    use crate::document::{DocumentModel, ObjectDoc, ObjectPayload};
    use crate::ecs::components::{AudioParams, TextContent};
    use crate::ecs::resources::{AudioPluginSettingsResource, SceneMeta, SystemSettingsResource};
    use crate::ecs::transform::Transform;
    use crate::export::{EncoderBackend, ExportCodec, ExportPreset};
    use crate::shortcuts::{CommandId, KeymapResource, Override, OwnedBinding, Scope};
    use std::collections::HashMap;

    #[test]
    fn protobuf_schema_contract_roundtrips_persisted_models() {
        let doc = DocumentModel {
            project_name: "schema-first".to_string(),
            audio_sample_rate: 48000,
            audio_channels: 2,
            active_scene: 0,
            next_object_id: 3,
            scenes: vec![SceneMeta::new(0, "Scene 1")],
            objects: vec![ObjectDoc {
                id: 1,
                scene_id: 0,
                kind_stable_id: "neoutl.object.text".to_string(),
                layer: 0,
                start_frame: 0,
                end_frame: 24,
                transform: Transform::default(),
                audio: AudioParams::default(),
                effects: Vec::new(),
                payload: ObjectPayload {
                    text: Some(TextContent::default()),
                    shape: None,
                    plugin_params: None,
                    media: None,
                    plugin_chain: None,
                    scene: None,
                    group_control: None,
                    clip_target: None,
                },
                keyframes: HashMap::new(),
            }],
        };
        let doc_schema = SchemaContract::to_schema(&doc);
        let doc_roundtrip: DocumentModel = SchemaContract::from_schema(&doc_schema).unwrap();
        assert_eq!(doc.project_name, doc_roundtrip.project_name);
        assert_eq!(doc.objects.len(), doc_roundtrip.objects.len());

        let settings = SystemSettingsResource::new();
        let settings_schema = SchemaContract::to_schema(&settings);
        let settings_roundtrip: SystemSettingsResource =
            SchemaContract::from_schema(&settings_schema).unwrap();
        assert_eq!(settings.theme_id, settings_roundtrip.theme_id);

        let plugin_settings = AudioPluginSettingsResource {
            scan_paths: vec!["/opt/plugins".to_string()],
            disabled_plugin_ids: vec!["com.example.plugin".to_string()],
            cached_catalog: Vec::new(),
            auto_detect_system: true,
        };
        let plugin_settings_schema = SchemaContract::to_schema(&plugin_settings);
        let plugin_settings_roundtrip: AudioPluginSettingsResource =
            SchemaContract::from_schema(&plugin_settings_schema).unwrap();
        assert_eq!(
            plugin_settings.scan_paths,
            plugin_settings_roundtrip.scan_paths
        );
        assert_eq!(
            plugin_settings.disabled_plugin_ids,
            plugin_settings_roundtrip.disabled_plugin_ids
        );

        let preset = ExportPreset {
            name: "schema roundtrip".to_string(),
            codec: ExportCodec::H264,
            backend: EncoderBackend::GpuVideo,
            average_bitrate: 8000000,
            max_bitrate: 12000000,
            container_ext: "mp4".to_string(),
        };
        let preset_schema = SchemaContract::to_schema(&preset);
        let preset_roundtrip: ExportPreset = SchemaContract::from_schema(&preset_schema).unwrap();
        assert_eq!(preset.name, preset_roundtrip.name);

        let keymap = KeymapResource {
            overrides: vec![Override {
                command: CommandId::SaveProject,
                scope: Scope::Global,
                binding: OwnedBinding {
                    ctrl: true,
                    shift: false,
                    alt: false,
                    key: "s".to_string(),
                },
            }],
        };
        let keymap_schema = SchemaContract::to_schema(&keymap);
        let keymap_roundtrip: KeymapResource = SchemaContract::from_schema(&keymap_schema).unwrap();
        assert_eq!(keymap.overrides.len(), keymap_roundtrip.overrides.len());
        assert_eq!(
            keymap.overrides[0].command,
            keymap_roundtrip.overrides[0].command
        );
    }
}
