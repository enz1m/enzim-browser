struct DocumentScript {
    script: enzui_webkit::page::UserScript,
    matches: Vec<String>,
}

impl DocumentScript {
    fn new(source: impl Into<String>, world: enzui_webkit::page::ScriptWorld) -> Self {
        Self {
            script: enzui_webkit::page::UserScript {
                source: source.into(),
                world,
                timing: enzui_webkit::page::ScriptTiming::DocumentStart,
                frames: enzui_webkit::page::ScriptFrames::TopFrame,
            },
            matches: Vec::new(),
        }
    }

    fn for_host(source: String, host: &str) -> Self {
        Self {
            matches: vec![format!("*://{host}/*")],
            ..Self::new(source, enzui_webkit::page::ScriptWorld::Page)
        }
    }

    fn install(
        &self,
        session: &enzui_webkit::ffi::runtime::WebSession,
    ) -> Result<(), enzui_webkit::page::ContentInjectionError> {
        if self.matches.is_empty() {
            session
                .add_user_script(&self.script)
                .map_err(enzui_webkit::page::ContentInjectionError::Script)
        } else {
            session.add_user_script_matching(&self.script, &self.matches, &[])
        }
    }
}
