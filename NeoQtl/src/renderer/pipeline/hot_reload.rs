use super::*;

impl RenderEngine {
    pub(super) fn drain_hot_reload_events(&mut self) {
        let Some(rx) = &self.hot_reload_rx else {
            return;
        };
        let events: Vec<ReloadEvent> = rx.try_iter().collect();
        for event in events {
            match event {
                ReloadEvent::Object(path) => self.apply_object_reload(&path),
                ReloadEvent::Effect(path) => self.apply_effect_reload(&path),
                ReloadEvent::Script(path) => self.apply_script_reload(&path),
            }
        }
    }

    fn apply_object_reload(&mut self, path: &std::path::Path) {
        if let Err(err) = crate::objects::loader::reload_one(path) {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] ホットリロード失敗（objects） %{arg0}: %{arg1}",
                    arg0 = format!("{}", path.display()),
                    arg1 = format!("{err}")
                )
            );
            return;
        }
        self.rebuild_all_object_pipelines();
    }

    fn apply_effect_reload(&mut self, path: &std::path::Path) {
        if let Err(err) = crate::effects::loader::reload_one(path) {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] ホットリロード失敗（effects） %{arg0}: %{arg1}",
                    arg0 = format!("{}", path.display()),
                    arg1 = format!("{err}")
                )
            );
            return;
        }
        self.rebuild_all_effect_pipelines();
    }

    fn apply_script_reload(&mut self, _path: &std::path::Path) {
        let Some(sys) = &self.lua_system else {
            return;
        };
        if let Err(err) = sys.reload_dir(&self.scripts_dir) {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] ホットリロード失敗（scripts） %{arg0}: %{arg1}",
                    arg0 = format!("{}", self.scripts_dir.display()),
                    arg1 = format!("{err}")
                )
            );
            return;
        }
        self.lua_compute_pipelines =
            build_lua_compute_pipelines(&self.device, &sys.drain_computes());
        crate::effects::loader::reload_lua(sys.drain_effects());
        self.rebuild_all_effect_pipelines();
        eprintln!(
            "{}",
            t!(
                "[NeoUtl] scriptsホットリロード完了: %{arg0}",
                arg0 = format!("{}", self.scripts_dir.display())
            )
        );
    }

    fn rebuild_all_object_pipelines(&mut self) {
        self.pipelines = build_pipelines_from_registry(&self.device, &self.object_pipeline_layout);
    }

    fn rebuild_all_effect_pipelines(&mut self) {
        self.effect_pipelines =
            build_effect_pipelines_from_registry(&self.device, &self.effect_pipeline_layout);
    }
}
