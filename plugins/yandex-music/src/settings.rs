// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Provider-owned prebuffer capacity, persisted through the existing settings API.
use crate::runtime::Runtime;
use arinn_plugin_sdk::runtime::SettingsHandler;
use arinn_protocol::{
    settings::{Setting, Text, Translations},
    validation::ErrorCode,
};
use serde_json::{Map, Value};
use std::{future::Future, pin::Pin, sync::Arc};
pub(crate) fn schema() -> Vec<Setting> {
    vec![Setting {
        id:"prefetch_tracks".into(),category:"playback".into(),title:"Preloaded tracks".into(),
        description:"Prepare upcoming tracks as soon as the current track loads. Audio stays in RAM; 0 disables preloading. Applies without interrupting playback.".into(),
        kind:"integer".into(),value:"5".into(),constraints:r#"{"min":0,"max":5}"#.into(),enabled:true,
    }]
}
pub(crate) fn count(values: &Map<String, Value>) -> Result<usize, ErrorCode> {
    values
        .get("prefetch_tracks")
        .and_then(Value::as_u64)
        .filter(|v| *v <= 5)
        .map(|v| v as usize)
        .ok_or(ErrorCode::InvalidArgument)
}
pub(crate) struct Settings(pub(crate) Arc<Runtime>);
impl SettingsHandler for Settings {
    fn schema(&self) -> Vec<Setting> {
        schema()
    }
    fn translations(&self) -> Translations {
        Translations::default().with("ru","prefetch_tracks",Text {
            title:Some("Треков в предзагрузке".into()),
            description:Some("Готовить следующие треки сразу после загрузки текущего. Аудио хранится в оперативной памяти; 0 отключает предзагрузку. Изменение не прерывает воспроизведение.".into()),..Text::default()
        })
    }
    fn apply(
        &self,
        values: Map<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move { self.0.set_prefetch_tracks(count(&values)?).await })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefetch_setting_defaults_to_five_and_rejects_out_of_range_or_malformed_values() {
        let setting = schema().remove(0);
        setting.validate().unwrap();
        assert_eq!(setting.value, "5");
        for value in [
            serde_json::json!(0),
            serde_json::json!(1),
            serde_json::json!(5),
        ] {
            let values = Map::from_iter([("prefetch_tracks".into(), value.clone())]);
            assert_eq!(count(&values).unwrap(), value.as_u64().unwrap() as usize);
        }
        for value in [
            serde_json::json!(-1),
            serde_json::json!(6),
            serde_json::json!(1.5),
            serde_json::json!("5"),
            Value::Null,
        ] {
            assert_eq!(
                count(&Map::from_iter([("prefetch_tracks".into(), value)])),
                Err(ErrorCode::InvalidArgument)
            );
        }
    }
}
