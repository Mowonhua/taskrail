//! 文件职责：以原子替换将 Taskrail MCP 写入所选宿主的配置文件。
use crate::storage::{staged_file, FileError};
use serde_json::{json, Value};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use super::agent_skill::{ensure_directory, host_directory, invalid, io_error, SkillAgent};

#[derive(Clone, Copy)]
pub(super) enum ServerEntry<'a> {
    Stdio {
        executable: &'a Path,
        profile: &'a Path,
        name: &'a str,
    },
    Http {
        url: &'a str,
    },
}

pub(super) struct PreparedConfig {
    pub path: PathBuf,
    previous: Option<String>,
    next: String,
}

impl PreparedConfig {
    pub(super) fn commit(&self) -> Result<(), FileError> {
        let file = staged_file(&self.path, self.next.as_bytes())?;
        if read_host_config(&self.path)? != self.previous {
            return Err(config_changed());
        }
        if self.previous.is_some() {
            file.persist(&self.path)
        } else {
            file.persist_noclobber(&self.path)
        }
        .map_err(|error| io_error(error.error))?;
        Ok(())
    }

    pub(super) fn rollback(&self) -> Result<(), FileError> {
        // 只撤销本次写入，避免覆盖其他程序同期修改的配置。
        if read_host_config(&self.path)?.as_deref() != Some(&self.next) {
            return Err(config_changed());
        }
        if let Some(previous) = &self.previous {
            staged_file(&self.path, previous.as_bytes())?
                .persist(&self.path)
                .map_err(|error| io_error(error.error))?;
        } else {
            fs::remove_file(&self.path).map_err(io_error)?;
        }
        Ok(())
    }
}

pub(super) fn prepare(
    root: &Path,
    agent: SkillAgent,
    global: bool,
    entry: ServerEntry<'_>,
) -> Result<PreparedConfig, FileError> {
    let path = match (agent, global) {
        (SkillAgent::ClaudeCode, false) => root.join(".mcp.json"),
        (SkillAgent::ClaudeCode, true) => root.join(".claude.json"),
        (SkillAgent::Shared, _) | (SkillAgent::Dsh, false) => return Err(invalid()),
        _ => {
            let directory = host_directory(root, agent, global)?;
            match agent {
                SkillAgent::Codex | SkillAgent::GrokBuild => directory.join("config.toml"),
                SkillAgent::Dsh => directory.join("cordis.patch.yml"),
                SkillAgent::Devin | SkillAgent::Antigravity => directory.join("mcp_config.json"),
                SkillAgent::Zcode if global => {
                    ensure_directory(&directory.join("cli"))?.join("config.json")
                }
                SkillAgent::Zcode => directory.join("config.json"),
                _ => directory.join("mcp.json"),
            }
        }
    };
    let previous = read_host_config(&path)?;
    let old = previous.as_deref().unwrap_or_default();
    let next = match agent {
        SkillAgent::Codex | SkillAgent::GrokBuild => toml_config(old, entry)?,
        SkillAgent::Dsh => dsh_config(old, entry)?,
        _ => json_config(old, root, agent, entry)?,
    };
    Ok(PreparedConfig {
        path,
        previous,
        next,
    })
}

fn invalid_config() -> FileError {
    FileError::new("MCP_CONFIG_INVALID", "Agent 宿主配置无法解析。")
}

fn existing() -> FileError {
    FileError::new("MCP_EXISTS", "Agent 宿主已配置 Taskrail MCP。")
}

fn config_changed() -> FileError {
    FileError::new("MCP_CONFIG_CHANGED", "Agent 宿主配置已更改，请重试安装。")
}

fn read_host_config(path: &Path) -> Result<Option<String>, FileError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(invalid());
                }
            }
            fs::read_to_string(path).map(Some).map_err(io_error)
        }
        Ok(_) => Err(invalid()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

fn stdio_arguments<'a>(
    executable: &'a Path,
    profile: &'a Path,
    name: &'a str,
) -> Result<(&'a str, [&'a str; 4]), FileError> {
    Ok((
        executable.to_str().ok_or_else(invalid)?,
        [
            "--config",
            profile.to_str().ok_or_else(invalid)?,
            "mcp",
            name,
        ],
    ))
}

fn toml_config(old: &str, entry: ServerEntry<'_>) -> Result<String, FileError> {
    use toml_edit::{value, Array, DocumentMut, Table, Value};
    let mut document = old.parse::<DocumentMut>().map_err(|_| invalid_config())?;
    if !document.contains_key("mcp_servers") {
        document["mcp_servers"] = toml_edit::table();
    }
    let servers = document["mcp_servers"]
        .as_table_like_mut()
        .ok_or_else(invalid_config)?;
    if servers.contains_key("taskrail") {
        return Err(existing());
    }
    let mut config = Table::new();
    match entry {
        ServerEntry::Http { url } => {
            config["url"] = value(url);
        }
        ServerEntry::Stdio {
            executable,
            profile,
            name,
        } => {
            let (command, args) = stdio_arguments(executable, profile, name)?;
            config["command"] = value(command);
            let args: Array = args.into_iter().collect();
            config["args"] = value(Value::Array(args));
        }
    }
    servers.insert("taskrail", toml_edit::Item::Table(config));
    Ok(document.to_string())
}

fn read_json(old: &str) -> Result<Value, FileError> {
    if old.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(old).map_err(|_| invalid_config())
    }
}

fn json_object<'a>(
    parent: &'a mut Value,
    key: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, FileError> {
    parent
        .as_object_mut()
        .ok_or_else(invalid_config)?
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(invalid_config)
}

fn json_config(
    old: &str,
    root: &Path,
    agent: SkillAgent,
    entry: ServerEntry<'_>,
) -> Result<String, FileError> {
    let mut document = read_json(old)?;
    let servers = if agent == SkillAgent::Zcode {
        json_object(&mut document, "mcp")?;
        let servers = json_object(&mut document["mcp"], "servers")?;
        // ZCode 发现原生服务器后会跳过整份兼容配置。首次写入时继承原来可用的服务器。
        if servers.is_empty() {
            let fallback = read_host_config(&root.join(".agents/mcp.json"))?;
            let fallback = read_json(fallback.as_deref().unwrap_or_default())?;
            if let Some(fallback) = fallback.get("mcpServers") {
                servers.extend(fallback.as_object().ok_or_else(invalid_config)?.clone());
            }
        }
        servers
    } else {
        json_object(&mut document, "mcpServers")?
    };
    if servers.contains_key("taskrail") {
        return Err(existing());
    }
    let config = match entry {
        ServerEntry::Http { url } => match agent {
            SkillAgent::ClaudeCode => json!({"type":"http","url":url}),
            SkillAgent::Antigravity => json!({"serverUrl":url}),
            SkillAgent::Devin => json!({"transport":"http","url":url}),
            SkillAgent::Zcode => {
                json!({"type":"http","url":url,"oauth":{"type":"authorization_code"}})
            }
            _ => json!({"url":url}),
        },
        ServerEntry::Stdio {
            executable,
            profile,
            name,
        } => {
            let (command, args) = stdio_arguments(executable, profile, name)?;
            let mut item = json!({"command":command,"args":args});
            if matches!(agent, SkillAgent::ClaudeCode | SkillAgent::Zcode) {
                item["type"] = json!("stdio");
            }
            item
        }
    };
    servers.insert("taskrail".into(), config);
    serde_json::to_string_pretty(&document).map_err(|_| invalid_config())
}

fn dsh_config(old: &str, entry: ServerEntry<'_>) -> Result<String, FileError> {
    use yaml_edit::YamlFile;

    let ServerEntry::Stdio {
        executable,
        profile,
        name,
    } = entry
    else {
        return Err(invalid());
    };
    let (command, args) = stdio_arguments(executable, profile, name)?;
    let mut document = old.parse::<YamlFile>().map_err(|_| invalid_config())?;
    if document.documents().next().is_none() {
        document = format!("{old}\n[]\n")
            .parse()
            .map_err(|_| invalid_config())?;
    }
    let mut documents = document.documents();
    let sequence = documents
        .next()
        .and_then(|doc| doc.as_sequence())
        .ok_or_else(invalid_config)?;
    if documents.next().is_some() {
        return Err(invalid_config());
    }
    if sequence.values().any(|patch| {
        is_dsh_taskrail(&patch)
            || patch
                .get("insert")
                .and_then(|insert| insert.as_sequence().cloned())
                .is_some_and(|rows| rows.values().any(|row| is_dsh_taskrail(&row)))
    }) {
        return Err(existing());
    }

    // JSON 是 YAML 的子集。路径和参数只作为字符串写入，已有 YAML 标签和注释保留。
    let patch = json!({"insert": [{
        "id": "mcp-taskrail",
        "name": "@deepseek-ai/dsh-mcp-client",
        "config": {"serverName":"taskrail","transport":"stdio","command":command,"args":args}
    }]});
    let new = format!("[{patch}]")
        .parse::<YamlFile>()
        .map_err(|_| invalid_config())?;
    let row = new
        .documents()
        .next()
        .and_then(|doc| doc.as_sequence())
        .and_then(|rows| rows.get(0))
        .ok_or_else(invalid_config)?;
    sequence.push(row);
    Ok(document.to_string())
}

fn is_dsh_taskrail(row: &yaml_edit::YamlNode) -> bool {
    row.get("id")
        .and_then(|id| id.as_scalar().map(|value| value.as_string()))
        .as_deref()
        == Some("mcp-taskrail")
        || row
            .get("config")
            .and_then(|config| config.get("serverName"))
            .and_then(|name| name.as_scalar().map(|value| value.as_string()))
            .as_deref()
            == Some("taskrail")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(
        root: &Path,
        agent: SkillAgent,
        global: bool,
        entry: ServerEntry<'_>,
    ) -> Result<PathBuf, FileError> {
        let config = prepare(root, agent, global, entry)?;
        config.commit()?;
        Ok(config.path)
    }

    #[test]
    fn writes_native_mcp_entries_for_new_harnesses() {
        for (agent, global, path) in [
            (SkillAgent::Devin, false, ".devin/mcp_config.json"),
            (SkillAgent::Antigravity, false, ".agents/mcp_config.json"),
            (
                SkillAgent::Antigravity,
                true,
                ".gemini/config/mcp_config.json",
            ),
            (SkillAgent::GrokBuild, false, ".grok/config.toml"),
            (SkillAgent::Zcode, false, ".zcode/config.json"),
            (SkillAgent::Zcode, true, ".zcode/cli/config.json"),
        ] {
            for http in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let root = root.path();
                let profile = root.join("private/connection.json");
                let executable = root.join("taskrail-agent");
                let url = "https://rail.example/mcp";
                let entry = if http {
                    ServerEntry::Http { url }
                } else {
                    ServerEntry::Stdio {
                        executable: &executable,
                        profile: &profile,
                        name: "test",
                    }
                };
                let actual = install(root, agent, global, entry).unwrap();
                assert!(actual.ends_with(path));
                let text = fs::read_to_string(actual).unwrap();
                if agent == SkillAgent::GrokBuild {
                    let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
                    let server = &doc["mcp_servers"]["taskrail"];
                    if http {
                        assert_eq!(server["url"].as_str(), Some(url));
                    } else {
                        assert_eq!(server["command"].as_str(), executable.to_str());
                        assert_eq!(server["args"].as_array().unwrap().len(), 4);
                    }
                    continue;
                }
                let doc = read_json(&text).unwrap();
                let server = if agent == SkillAgent::Zcode {
                    &doc["mcp"]["servers"]["taskrail"]
                } else {
                    &doc["mcpServers"]["taskrail"]
                };
                if http {
                    match agent {
                        SkillAgent::Antigravity => assert_eq!(server["serverUrl"], url),
                        SkillAgent::Devin => {
                            assert_eq!(server["transport"], "http");
                            assert_eq!(server["url"], url);
                        }
                        SkillAgent::Zcode => {
                            assert_eq!(server["url"], url);
                            assert_eq!(server["type"], "http");
                            assert_eq!(server["oauth"]["type"], "authorization_code");
                        }
                        _ => unreachable!(),
                    }
                } else {
                    assert_eq!(server["command"], executable.to_str().unwrap());
                    assert_eq!(
                        server["args"],
                        json!(["--config", profile.to_str().unwrap(), "mcp", "test"])
                    );
                }
            }
        }
    }

    #[test]
    fn preserves_zcode_compatibility_servers_on_first_native_install() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        fs::create_dir(root.join(".agents")).unwrap();
        fs::write(
            root.join(".agents/mcp.json"),
            r#"{"mcpServers":{"existing":{"command":"local"}}}"#,
        )
        .unwrap();
        let installed = install(
            root,
            SkillAgent::Zcode,
            false,
            ServerEntry::Http {
                url: "https://rail.example/mcp",
            },
        )
        .unwrap();
        let doc = read_json(&fs::read_to_string(installed).unwrap()).unwrap();
        assert_eq!(doc["mcp"]["servers"]["existing"]["command"], "local");
        assert_eq!(doc["mcp"]["servers"]["taskrail"]["type"], "http");
    }

    #[test]
    fn adds_dsh_overlay_without_replacing_comments_tags_or_existing_rows() {
        let original =
            "# local settings\n- id: existing\n  config:\n    path: !!js process.cwd()\n";
        let entry = || ServerEntry::Stdio {
            executable: Path::new("C:/Program Files/Taskrail/taskrail-agent.exe"),
            profile: Path::new("C:/User Data/connections/agent.json"),
            name: "test",
        };
        for old in ["", "# empty\n", "[]\n", original] {
            let updated = dsh_config(old, entry()).unwrap();
            if old == original {
                assert!(updated.contains(original));
            }
            let yaml = updated.parse::<yaml_edit::YamlFile>().unwrap();
            let sequence = yaml.documents().next().unwrap().as_sequence().unwrap();
            let patch = sequence.get(sequence.len() - 1).unwrap();
            let inserted = patch.get("insert").unwrap();
            let plugin = inserted.as_sequence().unwrap().get(0).unwrap();
            assert!(is_dsh_taskrail(&plugin));
            assert!(dsh_config(&updated, entry()).is_err());
        }
    }
}
