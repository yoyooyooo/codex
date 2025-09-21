use codex_modes::IndexMap;
use codex_modes::IndexSet;
use codex_modes::ModeDefinition;
use std::collections::HashMap;

/// 当前会话中已启用的持久模式状态快照。
#[derive(Debug, Clone, Default)]
pub(crate) struct PersistentModeState {
    pub enabled: IndexSet<String>,
    pub enable_order: Vec<String>,
    pub var_values: HashMap<String, IndexMap<String, Option<String>>>,
}

impl PersistentModeState {
    pub(crate) fn is_empty(&self) -> bool {
        self.enabled.is_empty()
    }

    pub(crate) fn sanitize(self, defs: &[ModeDefinition]) -> Self {
        let mut defs_by_id: HashMap<&str, &ModeDefinition> = HashMap::new();
        for def in defs {
            defs_by_id.insert(def.id.as_str(), def);
        }

        let mut enabled: IndexSet<String> = IndexSet::new();
        let mut enable_order: Vec<String> = Vec::new();
        for id in self.enable_order.into_iter() {
            if defs_by_id.contains_key(id.as_str()) && enabled.insert(id.clone()) {
                enable_order.push(id);
            }
        }
        for id in self.enabled.into_iter() {
            if defs_by_id.contains_key(id.as_str()) && enabled.insert(id.clone()) {
                enable_order.push(id);
            }
        }

        let mut var_values: HashMap<String, IndexMap<String, Option<String>>> = HashMap::new();
        for (mode_id, vars) in self.var_values.into_iter() {
            if let Some(def) = defs_by_id.get(mode_id.as_str()) {
                let mut filtered: IndexMap<String, Option<String>> = IndexMap::new();
                for (name, value) in vars {
                    if def.variables.iter().any(|v| v.name == name) {
                        filtered.insert(name, value);
                    }
                }
                if !filtered.is_empty() {
                    var_values.insert(mode_id, filtered);
                }
            }
        }

        Self {
            enabled,
            enable_order,
            var_values,
        }
    }
}
