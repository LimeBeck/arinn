// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed-copy translation tools. File-only commands never open D-Bus;
//! provider export only reads metadata and never launches/restarts a provider.
mod ts;

use anyhow::{Result, bail, ensure};
use arinn_protocol::{
    localization::{Catalog, TextsProxy, validate_locale},
    production::{self, ProductionDaemonProxy},
    settings::{self, SettingsApiProxy},
    validation,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn checked<T>(result: Result<T>) -> std::result::Result<T, u8> {
    result.map_err(|error| {
        eprintln!("Translation command: {}", terminal_text(&error.to_string()));
        1
    })
}
fn terminal_text(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}
fn valid_locale(locale: &str) -> std::result::Result<(), u8> {
    validate_locale(locale).map_err(|_| {
        eprintln!("Invalid language tag");
        2
    })
}
fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.into()
    } else {
        std::env::current_dir()?.join(path)
    })
}
fn user_root() -> Result<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let data = data.or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .map(|p| p.join(".local/share"))
    });
    Ok(data
        .ok_or_else(|| anyhow::anyhow!("No absolute user data directory"))?
        .join("arinn/translations"))
}
fn plugin_directory(plugin: &str) -> Result<String> {
    validation::id(plugin).map_err(|_| anyhow::anyhow!("Invalid provider identity"))?;
    Ok(
        if plugin != "."
            && plugin != ".."
            && plugin.len() <= 128
            && plugin
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            plugin.into()
        } else {
            format!("by-id/{:x}", Sha256::digest(plugin.as_bytes()))
        },
    )
}
fn destination(explicit: Option<&String>, locale: &str, relative: &str) -> Result<PathBuf> {
    if let Some(path) = explicit {
        absolute(Path::new(path))
    } else {
        Ok(user_root()?.join("overrides").join(locale).join(relative))
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Destination has no parent"))?,
    )?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn read(path: &Path, maximum: usize) -> Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "Translation file is too large");
    Ok(String::from_utf8(bytes)?)
}
async fn connection() -> std::result::Result<zbus::Connection, u8> {
    let connection = zbus::connection::Builder::session()
        .map_err(|_| 3)?
        .method_timeout(Duration::from_secs(3))
        .build()
        .await
        .map_err(|_| 3)?;
    let bus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|_| 3)?;
    if !bus
        .name_has_owner(production::SERVICE.try_into().unwrap())
        .await
        .map_err(|_| 3)?
    {
        return Err(3);
    }
    Ok(connection)
}
// Check the trusted registration before and after metadata export. An instance
// name may be reassigned to another installation while a provider is replying.
async fn identity(connection: &zbus::Connection, id: &str) -> Result<(String, String, String)> {
    let bus = zbus::fdo::DBusProxy::new(connection).await?;
    let owner = bus
        .get_name_owner(production::SERVICE.try_into().unwrap())
        .await?
        .to_string();
    let daemon = ProductionDaemonProxy::new(connection).await?;
    let provider = daemon
        .providers()
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow::anyhow!("Provider not found"))?;
    Ok((owner, provider.generation, provider.plugin_id))
}
fn settings_template(plugin: &str, locale: &str, schema: &[settings::Setting]) -> Result<String> {
    settings::validate_schema(schema)
        .map_err(|_| anyhow::anyhow!("Invalid provider settings schema"))?;
    let field = |source: &str| json!({"source": source, "translation": null});
    let mut entries = serde_json::Map::new();
    for setting in schema {
        let mut row =
            json!({"title": field(&setting.title), "description": field(&setting.description)});
        let constraints = setting
            .constraints()
            .map_err(|_| anyhow::anyhow!("Invalid setting constraints"))?;
        if let Some(unit) = constraints.unit.filter(|unit| !unit.is_empty()) {
            row["unit"] = field(&unit);
        }
        if !constraints.options.is_empty() {
            row["options"] = Value::Object(
                constraints
                    .options
                    .iter()
                    .map(|option| (option.value.clone(), field(&option.title)))
                    .collect(),
            );
        }
        entries.insert(setting.id.clone(), row);
    }
    let result = serde_json::to_string_pretty(
        &json!({"formatVersion":1,"pluginId":plugin,"locale":locale,"settings":entries}),
    )?;
    check_settings(&result)?;
    Ok(result + "\n")
}
fn check_settings(input: &str) -> Result<Value> {
    ensure!(
        input.len() <= 256 * 1024,
        "Provider settings overlay exceeds 256 KiB"
    );
    let object: Value = serde_json::from_str(input)?;
    let root = object
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Expected a settings object"))?;
    ensure!(
        root.keys()
            .all(|key| ["formatVersion", "pluginId", "locale", "settings"].contains(&key.as_str())),
        "Unknown settings-envelope field"
    );
    ensure!(
        object["formatVersion"].as_u64() == Some(1),
        "Unsupported settings format"
    );
    plugin_directory(object["pluginId"].as_str().unwrap_or_default())?;
    let locale = object["locale"].as_str().unwrap_or_default();
    validate_locale(locale).map_err(|_| anyhow::anyhow!("Invalid settings language"))?;
    let entries = object["settings"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Missing settings map"))?;
    let mut finished = 0;
    let mut unfinished = 0;
    let mut field = |value: &Value, description: bool| -> Result<()> {
        let value = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Expected a source/translation pair"))?;
        ensure!(
            value
                .keys()
                .all(|key| ["source", "translation"].contains(&key.as_str())),
            "Unknown translated-field property"
        );
        let valid_text = |s: &str, empty: bool| {
            (empty || !s.is_empty())
                && s.len() <= 8192
                && !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
        };
        ensure!(
            value
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|s| valid_text(s, description)),
            "Invalid expected source"
        );
        match value.get("translation") {
            None | Some(Value::Null) => unfinished += 1,
            Some(Value::String(s)) if valid_text(s, description) => finished += 1,
            _ => bail!("Invalid translated settings text"),
        }
        Ok(())
    };
    for (id, entry) in entries {
        validation::id(id).map_err(|_| anyhow::anyhow!("Invalid setting id"))?;
        for (name, value) in entry
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Invalid setting entry"))?
        {
            if name == "options" {
                for value in value
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("Invalid options map"))?
                    .values()
                {
                    field(value, false)?;
                }
            } else {
                ensure!(
                    ["title", "description", "unit"].contains(&name.as_str()),
                    "Unknown setting text field"
                );
                field(value, name == "description")?;
            }
        }
    }
    Ok(
        json!({"locale":locale,"finished":finished,"unfinished":unfinished,"component":"providerSettings"}),
    )
}
fn check_file(path: &Path) -> Result<Value> {
    let input = read(path, ts::MAX_TS_BYTES)?;
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        check_settings(&input)
    } else {
        Ok(serde_json::to_value(ts::check(&ts::parse(&input)?)?)?)
    }
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn self_contained_qm(bytes: &[u8]) -> bool {
    const MAGIC: &[u8] = b"\x3c\xb8\x64\x18\xca\xef\x9c\x95\xcd\x21\x1c\xbf\x60\xa1\xbd\xdd";
    if !bytes.starts_with(MAGIC) {
        return false;
    }
    let mut at = MAGIC.len();
    while at < bytes.len() {
        if bytes.len() - at < 5 || bytes[at] == 0x96 {
            return false;
        }
        let size = u32::from_be_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize;
        at += 5;
        if size > bytes.len() - at {
            return false;
        }
        at += size;
    }
    true
}
fn compile(input: &Path, output: Option<&String>) -> Result<Value> {
    let candidates = ["lrelease6", "lrelease", "/usr/lib/qt6/bin/lrelease"].map(PathBuf::from);
    compile_with_candidates(input, output, &candidates)
}
fn compile_with_candidates(
    input: &Path,
    output: Option<&String>,
    candidates: &[PathBuf],
) -> Result<Value> {
    let input = input.canonicalize()?;
    let snapshot = read(&input, ts::MAX_TS_BYTES)?;
    let report = ts::check(&ts::parse(&snapshot)?)?;
    let output = output.map_or_else(
        || Ok(input.with_extension("qm")),
        |path| absolute(Path::new(path)),
    )?;
    ensure!(
        output.canonicalize().unwrap_or_else(|_| output.clone()) != input,
        "Compiled output must differ from the editable source"
    );
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Output has no parent"))?;
    fs::create_dir_all(parent)?;
    // Creating a previously missing parent can make a `new/../source.ts`
    // destination resolve to the source. Check again after resolving it.
    let output = parent.canonicalize()?.join(
        output
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Output is not a file"))?,
    );
    ensure!(
        output.canonicalize().unwrap_or_else(|_| output.clone()) != input,
        "Compiled output must differ from the editable source"
    );
    let compiler = candidates
        .iter()
        .find(|program| {
            Command::new(program)
                .arg("-version")
                .output()
                .is_ok_and(|result| {
                    result.status.success()
                        && String::from_utf8_lossy(&result.stdout).contains("version 6.")
                })
        })
        .ok_or_else(|| anyhow::anyhow!("Qt 6 Linguist lrelease is required"))?;
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Output has no parent"))?;
    let temporary =
        Temporary(parent.join(format!(".arinn-translation-{}.qm", uuid::Uuid::new_v4())));
    // Qt Linguist/editor saves may replace the original after validation.
    // Give the compiler the exact checked bytes, never reopen that path.
    let source_snapshot =
        Temporary(parent.join(format!(".arinn-source-{}.ts", uuid::Uuid::new_v4())));
    write_new(&source_snapshot.0, snapshot.as_bytes())?;
    write_new(&temporary.0, b"")?;
    let mut command = Command::new(compiler);
    command.arg("-nounfinished");
    if report.id_based {
        command.arg("-idbased");
    }
    let result = command
        .arg(&source_snapshot.0)
        .arg("-qm")
        .arg(&temporary.0)
        .output()?;
    ensure!(result.status.success(), "lrelease failed");
    let maximum = if report.id_based {
        240 * 1024
    } else {
        1024 * 1024
    };
    let mut bytes = Vec::new();
    fs::File::open(&temporary.0)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= maximum && self_contained_qm(&bytes),
        "Invalid, oversized or dependent compiled catalogue"
    );
    fs::File::open(&temporary.0)?.sync_all()?;
    fs::rename(&temporary.0, &output)?;
    Ok(
        json!({"locale":report.locale,"finished":report.finished,"unfinished":report.unfinished,"output":output}),
    )
}

pub async fn run(args: &[String], json_output: bool) -> std::result::Result<(), u8> {
    let result = match args.first().map(String::as_str) {
        Some("export-shell") => {
            valid_locale(&args[1])?;
            if !["arinn", "music"].contains(&args[2].as_str()) {
                return Err(2);
            }
            let source = checked(ts::shell(&args[2], &args[1]))?;
            let path = checked(destination(
                args.get(3),
                &args[1],
                &format!("shell/{}.ts", args[2]),
            ))?;
            checked(write_new(&path, source.as_bytes()))?;
            json!({"output":path,"locale":args[1],"component":args[2]})
        }
        Some("export-provider" | "export-settings") => {
            validation::id(&args[1]).map_err(|_| 2)?;
            valid_locale(&args[2])?;
            let connection = connection().await?;
            let before = checked(identity(&connection, &args[1]).await)?;
            let messages = args[0] == "export-provider";
            let source = if messages {
                let api = TextsProxy::new(&connection).await.map_err(super::failed)?;
                let (generation, encoded) =
                    api.get_catalog(&args[1]).await.map_err(super::failed)?;
                let catalog = Catalog::from_json(&encoded).map_err(|_| 1)?;
                if generation != before.1 || catalog.plugin_id != before.2 {
                    eprintln!("Conflict");
                    return Err(1);
                }
                checked(ts::provider(&catalog, &args[2]))?
            } else {
                let api = SettingsApiProxy::new(&connection)
                    .await
                    .map_err(super::failed)?;
                let (generation, schema) =
                    api.get_settings(&args[1]).await.map_err(super::failed)?;
                if generation != before.1 {
                    eprintln!("Conflict");
                    return Err(1);
                }
                checked(settings_template(&before.2, &args[2], &schema))?
            };
            if checked(identity(&connection, &args[1]).await)? != before {
                eprintln!("Conflict");
                return Err(1);
            }
            let relative = format!(
                "providers/{}/{}",
                checked(plugin_directory(&before.2))?,
                if messages {
                    "messages.ts"
                } else {
                    "settings.json"
                }
            );
            let path = checked(destination(args.get(3), &args[2], &relative))?;
            checked(write_new(&path, source.as_bytes()))?;
            json!({"output":path,"locale":args[2],"pluginId":before.2})
        }
        Some("check") => checked(check_file(Path::new(&args[1])))?,
        Some("compile") => checked(compile(Path::new(&args[1]), args.get(2)))?,
        _ => return Err(2),
    };
    let mut display = result;
    if !json_output && let Some(fields) = display.as_object_mut() {
        for value in fields.values_mut() {
            if let Value::String(text) = value {
                *text = terminal_text(text);
            }
        }
    }
    super::output(&display, json_output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiler_uses_checked_bytes_even_if_a_probe_replaces_the_editable_source() {
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("arinn-translation-race-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let input = directory.join("input.ts");
        let valid = "<TS language='de'><context><name>X</name><message><source>%1</source><translation>%1</translation></message></context></TS>";
        fs::write(&input, valid).unwrap();
        let compiler = directory.join("fake-compiler");
        fs::write(
            &compiler,
            r#"#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
import sys
from pathlib import Path
root=Path(__file__).parent
if sys.argv[1:] == ['-version']:
    (root/'input.ts').write_text('unchecked replacement')
    print('lrelease version 6.99')
else:
    at=sys.argv.index('-qm')
    (root/'compiled-input.ts').write_bytes(Path(sys.argv[at-1]).read_bytes())
    Path(sys.argv[at+1]).write_bytes(bytes.fromhex('3cb86418caef9c95cd211cbf60a1bddd'))
"#,
        )
        .unwrap();
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700)).unwrap();
        compile_with_candidates(&input, None, &[compiler]).unwrap();
        assert_eq!(fs::read_to_string(&input).unwrap(), "unchecked replacement");
        assert_eq!(
            fs::read_to_string(directory.join("compiled-input.ts")).unwrap(),
            valid
        );
        assert!(!fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".arinn-")
        }));
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn opaque_plugin_paths_match_shell_hashing_and_cannot_escape_the_root() {
        assert_eq!(plugin_directory("radio.example").unwrap(), "radio.example");
        for plugin in ["..", "../radio", "third.party/radio", "радио", "\u{1}radio"] {
            let path = plugin_directory(plugin).unwrap();
            assert!(path.starts_with("by-id/"));
            assert_eq!(path.len(), 6 + 64);
        }
    }
    #[test]
    fn settings_export_never_copies_values_or_hides_unfinished_descriptions() {
        let schema = [settings::Setting {
            id: "secret".into(),
            category: "system".into(),
            title: "API key".into(),
            description: "Service credential".into(),
            kind: "string".into(),
            value: "\"PRIVATE_SENTINEL\"".into(),
            constraints: "{}".into(),
            enabled: true,
        }];
        let source = settings_template("example", "de", &schema).unwrap();
        assert!(!source.contains("PRIVATE_SENTINEL"));
        let value: Value = serde_json::from_str(&source).unwrap();
        assert!(value["settings"]["secret"]["description"]["translation"].is_null());
        assert_eq!(check_settings(&source).unwrap()["finished"], 0);
    }
    #[test]
    fn unfinished_settings_and_intentionally_empty_descriptions_are_distinct() {
        let mut value = json!({"formatVersion":1,"pluginId":"example","locale":"de","settings":{"x":{"description":{"source":"Description","translation":null}}}});
        assert_eq!(check_settings(&value.to_string()).unwrap()["unfinished"], 1);
        value["settings"]["x"]["description"]["translation"] = "".into();
        assert_eq!(check_settings(&value.to_string()).unwrap()["finished"], 1);
        value["settings"]["x"]["title"] = json!({"source":"Title","translation":""});
        assert!(check_settings(&value.to_string()).is_err());
    }
    #[test]
    fn export_and_invalid_compilation_preserve_existing_user_files() {
        let directory =
            std::env::temp_dir().join(format!("arinn-translation-files-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let source = directory.join("arinn_de.ts");
        let output = directory.join("arinn_de.qm");
        fs::write(&source, "<TS language='de'><context><name>X</name><message><source>%1</source><translation>%2</translation></message></context></TS>").unwrap();
        fs::write(&output, b"existing compiled catalogue").unwrap();
        let before = fs::read(&source).unwrap();
        assert!(write_new(&source, b"replacement").is_err());
        assert_eq!(fs::read(&source).unwrap(), before);
        assert!(compile(&source, None).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"existing compiled catalogue");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        let valid = ts::shell("arinn", "de").unwrap();
        fs::write(&source, &valid).unwrap();
        let alias = directory
            .join("new/../arinn_de.ts")
            .to_string_lossy()
            .into_owned();
        let error = compile_with_candidates(&source, Some(&alias), &[]).unwrap_err();
        assert!(error.to_string().contains("must differ"));
        assert_eq!(fs::read_to_string(&source).unwrap(), valid);
        fs::remove_dir_all(directory).unwrap();
    }
}
