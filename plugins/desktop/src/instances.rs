// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::discovery::Entry;
use arinn_plugin_sdk::platform::WindowSnapshot;
use arinn_protocol::application::RunningApplication;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub struct Instances {
    generation: String,
    foreground: String,
    sequence: u64,
    recent: HashMap<String, u64>,
}
impl Instances {
    pub fn reconcile(
        &mut self,
        entries: &BTreeMap<String, Entry>,
        snapshot: &WindowSnapshot,
    ) -> Vec<RunningApplication> {
        if self.generation != snapshot.generation {
            *self = Self {
                generation: snapshot.generation.clone(),
                ..Self::default()
            };
        }
        let live: HashSet<_> = snapshot.windows.iter().map(|w| &w.token).collect();
        self.recent.retain(|token, _| live.contains(token));
        let foreground = snapshot
            .windows
            .iter()
            .find(|w| w.active)
            .map(|w| w.token.as_str())
            .unwrap_or("");
        if self.foreground != foreground {
            self.sequence = self.sequence.saturating_add(1);
            self.foreground = foreground.into();
            if !foreground.is_empty() {
                self.recent.insert(foreground.into(), self.sequence);
            }
        }
        let mut running: Vec<_> = snapshot
            .windows
            .iter()
            .filter_map(|w| {
                let entry = entries.get(&w.desktop_id)?;
                Some(RunningApplication {
                    id: w.token.clone(),
                    application_id: entry.application.id.clone(),
                    title: entry.application.title.clone(),
                    window_token: w.token.clone(),
                    can_activate: w.can_activate,
                    can_terminate: w.can_close,
                })
            })
            .collect();
        running.sort_by(|a, b| {
            self.recent
                .get(&b.id)
                .copied()
                .unwrap_or(0)
                .cmp(&self.recent.get(&a.id).copied().unwrap_or(0))
                .then_with(|| a.id.cmp(&b.id))
        });
        running
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use arinn_protocol::{application::Application, platform::Window};
    #[test]
    fn exact_association_multiple_windows_and_remap() {
        let entries = BTreeMap::from([(
            "app.desktop".into(),
            Entry {
                application: Application {
                    id: "app.desktop".into(),
                    title: "App".into(),
                    can_launch: true,
                    group: "other".into(),
                    icon: String::new(),
                },
                path: "/fixture/app.desktop".into(),
                dbus_name: None,
            },
        )]);
        let window = |token: &str, desktop: &str, active| Window {
            token: token.into(),
            desktop_id: desktop.into(),
            output: String::new(),
            active,
            fullscreen: false,
            can_activate: true,
            can_close: false,
        };
        let mut snapshot = WindowSnapshot {
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            windows: vec![
                window("a", "app.desktop", false),
                window("b", "app.desktop", true),
                window("unknown", "app", false),
            ],
        };
        let mut model = Instances::default();
        let running = model.reconcile(&entries, &snapshot);
        assert_eq!(
            running.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(!running[0].can_terminate);
        snapshot.windows[1].active = false;
        snapshot.windows[0].active = true;
        snapshot.revision += 1;
        assert_eq!(model.reconcile(&entries, &snapshot)[0].id, "a");
        snapshot.windows.remove(0);
        snapshot
            .windows
            .push(window("remapped", "app.desktop", false));
        snapshot.revision += 1;
        assert!(
            !model
                .reconcile(&entries, &snapshot)
                .iter()
                .any(|r| r.id == "a")
        );
        snapshot.generation = uuid::Uuid::new_v4().to_string();
        model.reconcile(&entries, &snapshot);
        assert!(model.recent.is_empty());
    }
}
