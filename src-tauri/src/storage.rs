//! 文件职责：实现与 UI 无关的配置持久化与路径身份工具。
//! 定义范围：稳定错误码、原子 JSON 保存、路径归一化和内容摘要。
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::{fs, io::Write};

const MAX_THEME_EXPORT_BYTES: usize = 262144;
const MAX_MARKDOWN_EXPORT_BYTES: usize = 1024 * 1024;

/// 结构职责：向调用方提供可稳定分支处理的错误。
/// 字段说明：code 与本地语言无关，message 包含操作诊断。
#[derive(Debug, Clone, Serialize)]
pub struct FileError {
    pub code: &'static str,
    pub message: String,
}

/// 函数职责：把用户选择的文件归一为不含目录别名的稳定绝对路径。
/// 输入说明：新文件仅允许最终文件名缺失，父目录必须已存在。
/// 输出说明：Windows 返回普通盘符或 UNC 路径，避免扩展路径前缀产生重复关联。
/// 实现思路：现存文件解析目标；新文件解析父目录后拼接最终文件名。
pub fn canonical_path(path: &Path, create: bool) -> Result<String, FileError> {
    let resolved = match fs::canonicalize(path) {
        Ok(resolved) => resolved,
        Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or_else(|| FileError::new("FILE_PATH", "文件名无效。"))?;
            let parent = path
                .parent()
                .filter(|value| !value.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::canonicalize(parent).map_err(FileError::io)?.join(name)
        }
        Err(error) => return Err(FileError::io(error)),
    };
    let identity = resolved.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        if let Some(unc) = identity.strip_prefix(r"\\?\UNC\") {
            return Ok(format!(r"\\{unc}"));
        }
        if let Some(local) = identity.strip_prefix(r"\\?\") {
            return Ok(local.to_owned());
        }
    }
    Ok(identity)
}

impl FileError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn io(error: std::io::Error) -> Self {
        let code = match error.kind() {
            std::io::ErrorKind::NotFound => "FILE_NOT_FOUND",
            std::io::ErrorKind::PermissionDenied => "FILE_PERMISSION",
            std::io::ErrorKind::AlreadyExists => "FILE_EXISTS",
            _ => "FILE_IO",
        };
        Self::new(code, error.to_string())
    }
}

/// 对完整字节计算稳定摘要，供单实例识别生成固定长度的身份键。
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 归一化用于单实例识别的可执行文件路径；Windows 忽略大小写并统一分隔符。
/// 此函数不访问磁盘，路径解析由调用方完成。
pub fn file_identity(path: &str) -> String {
    let identity = path.replace('\\', "/");
    #[cfg(windows)]
    let identity = identity.to_lowercase();
    identity
}

/// 函数职责：读取可选文件的完整字节，并严格区分缺失与 IO 错误。
/// 输入说明：路径可以位于尚未创建的应用状态目录。
/// 输出说明：仅 NotFound 返回 None，其他错误原样映射为稳定文件错误。
/// 实现思路：读取原始字节，供配置解析为 JSON。
pub fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, FileError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(FileError::io(error)),
    }
}

/// 临时文件必须与目标同目录，写完先刷新内容再替换，避免跨卷重命名和部分正文可见。
fn staged_file(path: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile, FileError> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(FileError::io)?;
    temporary.write_all(bytes).map_err(FileError::io)?;
    temporary.as_file().sync_all().map_err(FileError::io)?;
    Ok(temporary)
}

/// 函数职责：在文件提交后刷新父目录项，保证目录变更在 Unix 上持久化。
/// 调用约束：调用前必须已刷新文件内容并完成提交；Windows 的原子替换由 tempfile 完成，此处不再操作目录。
pub(crate) fn sync_parent(path: &Path) -> Result<(), FileError> {
    #[cfg(unix)]
    fs::File::open(path.parent().unwrap_or(Path::new(".")))
        .and_then(|directory| directory.sync_all())
        .map_err(FileError::io)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// 函数职责：原子写入应用私有 JSON 状态。
/// 输入说明：目录由应用数据目录派生，不接收任意配置路径。
/// 输出说明：失败时原配置继续存在。
/// 实现思路：复用已刷新临时文件的原子替换能力。
pub fn save_json(path: &Path, value: &serde_json::Value) -> Result<(), FileError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FileError::io)?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| FileError::new("STATE_INVALID", error.to_string()))?;
    staged_file(path, &bytes)?
        .persist(path)
        .map_err(|error| FileError::io(error.error))?;
    sync_parent(path)
}

/// 函数职责：校验主题导出输入，并原子保存原始 UTF-8 JSON 文本。
/// 输入说明：仅接受 .json 扩展名（不区分大小写）和不超过 256 KiB 的合法 JSON；父目录必须存在，已有目标必须是普通文件。
/// 输出说明：拒绝目录、符号链接及悬空链接；校验失败不修改目标，写入成功时可替换已有普通文件。
pub fn export_theme_file(path: &Path, text: &str) -> Result<(), FileError> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err(FileError::new(
            "THEME_EXPORT_PATH",
            "主题导出文件必须使用 .json 扩展名。",
        ));
    }
    if text.len() > MAX_THEME_EXPORT_BYTES {
        return Err(FileError::new(
            "THEME_EXPORT_SIZE",
            "主题导出内容不能超过 256 KiB。",
        ));
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map_err(|error| FileError::new("THEME_EXPORT_INVALID", error.to_string()))?;

    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // 只解析父目录，保留最终目录项供 symlink_metadata 检查，避免把符号链接解析成普通文件。
    let parent = fs::canonicalize(parent)
        .map_err(|error| FileError::new("THEME_EXPORT_PATH", error.to_string()))?;
    if !parent.is_dir() {
        return Err(FileError::new(
            "THEME_EXPORT_PATH",
            "主题导出目标的父路径必须是已存在的目录。",
        ));
    }
    let name = path
        .file_name()
        .ok_or_else(|| FileError::new("THEME_EXPORT_PATH", "主题导出文件名无效。"))?;
    let target = parent.join(name);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(FileError::new(
                "THEME_EXPORT_PATH",
                "主题导出目标必须是普通文件，不能是目录或符号链接。",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(FileError::io(error)),
    }
    // persist 替换目标目录项而不打开目标内容，避免写入期间跟随被外部替换的符号链接。
    staged_file(&target, text.as_bytes())?
        .persist(&target)
        .map_err(|error| FileError::io(error.error))?;
    sync_parent(&target)
}

/// 导出原始 UTF-8 文本，仅接受 .md、.markdown、.txt，最多 1 MiB。
/// 调用方必须在 FileState.gate 内执行；父目录必须存在，已有目标必须是普通文件。
pub fn export_markdown_file(path: &Path, text: &str) -> Result<(), FileError> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["md", "markdown", "txt"]
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
    {
        return Err(FileError::new(
            "MARKDOWN_EXPORT_PATH",
            "导出文件必须使用 .md、.markdown 或 .txt 扩展名。",
        ));
    }
    if text.len() > MAX_MARKDOWN_EXPORT_BYTES {
        return Err(FileError::new(
            "MARKDOWN_EXPORT_SIZE",
            "导出内容不能超过 1 MiB。",
        ));
    }
    // Windows 的备用数据流可伪装扩展名；盘符前缀不属于 Normal 组件。
    #[cfg(windows)]
    if path.components().any(|component| matches!(component, std::path::Component::Normal(name) if name.to_string_lossy().contains(':'))) {
        return Err(FileError::new("MARKDOWN_EXPORT_PATH", "导出路径不能包含备用数据流。"));
    }
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // 仅解析父目录，保留最终目录项，以便识别现存及悬空符号链接。
    let parent = fs::canonicalize(parent)
        .ok()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| {
            FileError::new(
                "MARKDOWN_EXPORT_PATH",
                "导出目标的父路径必须是已存在的目录。",
            )
        })?;
    let name = path
        .file_name()
        .ok_or_else(|| FileError::new("MARKDOWN_EXPORT_PATH", "导出文件名无效。"))?;
    let target = parent.join(name);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(FileError::new(
                "MARKDOWN_EXPORT_PATH",
                "导出目标必须是普通文件，不能是目录或符号链接。",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(FileError::new("MARKDOWN_EXPORT_PATH", "无法检查导出目标。"));
        }
    }
    // 同目录临时文件先刷新再替换目录项，避免部分正文可见或跟随目标符号链接。
    let write_error = |_| FileError::new("MARKDOWN_EXPORT_WRITE", "无法写入导出文件。");
    staged_file(&target, text.as_bytes())
        .map_err(write_error)?
        .persist(&target)
        .map_err(|error| write_error(FileError::io(error.error)))?;
    sync_parent(&target).map_err(write_error)
}

/// 函数职责：读取可选的应用私有 JSON 状态。
/// 输入说明：路径来自应用配置数据目录。
/// 输出说明：仅缺失返回 None；损坏和权限错误不伪装为空配置。
/// 实现思路：读取后解析 JSON，独立映射错误。
pub fn load_json(path: &Path) -> Result<Option<serde_json::Value>, FileError> {
    let Some(bytes) = read_optional_bytes(path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| FileError::new("STATE_INVALID", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_export_preserves_utf8_text_and_replaces_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["SKILL.md", "guide.MD", "guide.MarkDown", "guide.TXT"] {
            let path = directory.path().join(name);
            let text = "# 接入指南\r\n\n正文\n";
            export_markdown_file(&path, text).unwrap();
            assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
            export_markdown_file(&path, "替换正文").unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "替换正文");
            export_markdown_file(&path, "").unwrap();
            assert!(fs::read(&path).unwrap().is_empty());
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 4);
    }

    #[test]
    fn markdown_export_rejects_invalid_extensions_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["SKILL", ".md", "guide.json", "guide.md.exe", "guide."] {
            let path = directory.path().join(name);
            fs::write(&path, "original").unwrap();
            assert_eq!(
                export_markdown_file(&path, "replacement").unwrap_err().code,
                "MARKDOWN_EXPORT_PATH"
            );
            assert_eq!(fs::read_to_string(path).unwrap(), "original");
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 5);
    }

    #[cfg(windows)]
    #[test]
    fn markdown_export_rejects_alternate_data_stream_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original.exe");
        fs::write(&path, "original").unwrap();
        let stream = directory.path().join("original.exe:guide.md");
        assert_eq!(
            export_markdown_file(&stream, "text").unwrap_err().code,
            "MARKDOWN_EXPORT_PATH"
        );
        assert!(fs::read(stream).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "original");
    }

    #[test]
    fn markdown_export_accepts_the_byte_limit_and_rejects_larger_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("SKILL.md");
        let at_limit = "a".repeat(1024 * 1024);
        export_markdown_file(&path, &at_limit).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), at_limit);

        for oversized in [format!("{at_limit}a"), "界".repeat(350000)] {
            assert_eq!(
                export_markdown_file(&path, &oversized).unwrap_err().code,
                "MARKDOWN_EXPORT_SIZE"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), at_limit);
            let missing = directory.path().join("missing.md");
            assert_eq!(
                export_markdown_file(&missing, &oversized).unwrap_err().code,
                "MARKDOWN_EXPORT_SIZE"
            );
            assert!(!missing.exists());
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn markdown_export_rejects_missing_parent_and_parent_that_is_a_file() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        let file = directory.path().join("file");
        fs::write(&file, "original").unwrap();
        for parent in [&missing, &file] {
            assert_eq!(
                export_markdown_file(&parent.join("SKILL.md"), "text")
                    .unwrap_err()
                    .code,
                "MARKDOWN_EXPORT_PATH"
            );
        }
        assert!(!missing.exists());
        assert_eq!(fs::read_to_string(file).unwrap(), "original");
    }

    #[test]
    fn markdown_export_rejects_directory_targets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("SKILL.md");
        fs::create_dir(&path).unwrap();
        let child = path.join("keep.txt");
        fs::write(&child, "original").unwrap();
        assert_eq!(
            export_markdown_file(&path, "text").unwrap_err().code,
            "MARKDOWN_EXPORT_PATH"
        );
        assert_eq!(fs::read_to_string(child).unwrap(), "original");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn markdown_export_rejects_existing_and_dangling_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("original.md");
        fs::write(&target, "original").unwrap();
        for (name, destination) in [
            ("existing.md", target.clone()),
            ("dangling.md", directory.path().join("missing.md")),
        ] {
            let link = directory.path().join(name);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&destination, &link).unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&destination, &link).unwrap();
            assert_eq!(
                export_markdown_file(&link, "text").unwrap_err().code,
                "MARKDOWN_EXPORT_PATH"
            );
            assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        }
        assert_eq!(fs::read_to_string(target).unwrap(), "original");
        assert!(!directory.path().join("missing.md").exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
    }

    #[test]
    fn theme_export_preserves_utf8_text_and_replaces_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("theme.JSON");
        let text = "{\n  \"name\": \"纸面\"\n}\n";

        export_theme_file(&path, text).unwrap();
        assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
        export_theme_file(&path, "{\"name\":\"深色\"}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"name\":\"深色\"}");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn theme_export_rejects_non_json_extensions_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["theme.txt", "theme.json.exe", "theme"] {
            let path = directory.path().join(name);
            assert_eq!(
                export_theme_file(&path, "{}").unwrap_err().code,
                "THEME_EXPORT_PATH"
            );
            assert!(!path.exists());
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn theme_export_rejects_invalid_json_and_preserves_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("theme.json");
        let original = "{\"name\":\"原主题\"}";
        fs::write(&path, original).unwrap();

        for text in ["", "{", "{} trailing", "{\"name\":}"] {
            assert_eq!(
                export_theme_file(&path, text).unwrap_err().code,
                "THEME_EXPORT_INVALID"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn theme_export_accepts_the_byte_limit_and_rejects_larger_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("theme.json");
        let at_limit = format!("\"{}\"", "a".repeat(262144 - 2));
        export_theme_file(&path, &at_limit).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 262144);

        for oversized in [
            format!("\"{}\"", "a".repeat(262144 - 1)),
            format!("\"{}\"", "界".repeat(90000)),
        ] {
            assert_eq!(
                export_theme_file(&path, &oversized).unwrap_err().code,
                "THEME_EXPORT_SIZE"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), at_limit);
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn theme_export_rejects_missing_parent_and_parent_that_is_a_file() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        let file = directory.path().join("file");
        fs::write(&file, "original").unwrap();

        for parent in [&missing, &file] {
            assert_eq!(
                export_theme_file(&parent.join("theme.json"), "{}")
                    .unwrap_err()
                    .code,
                "THEME_EXPORT_PATH"
            );
        }
        assert!(!missing.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn theme_export_rejects_directory_targets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("theme.json");
        fs::create_dir(&path).unwrap();
        let child = path.join("keep.txt");
        fs::write(&child, "original").unwrap();

        assert_eq!(
            export_theme_file(&path, "{}").unwrap_err().code,
            "THEME_EXPORT_PATH"
        );
        assert_eq!(fs::read_to_string(child).unwrap(), "original");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn theme_export_rejects_existing_and_dangling_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("original.json");
        fs::write(&target, "original").unwrap();

        for (name, destination) in [
            ("existing.json", target.clone()),
            ("dangling.json", directory.path().join("missing.json")),
        ] {
            let link = directory.path().join(name);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&destination, &link).unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&destination, &link).unwrap();

            assert_eq!(
                export_theme_file(&link, "{}").unwrap_err().code,
                "THEME_EXPORT_PATH"
            );
            assert!(fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink());
        }
        assert_eq!(fs::read_to_string(target).unwrap(), "original");
        assert!(!directory.path().join("missing.json").exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
    }

    #[test]
    fn canonical_identity_resolves_parent_segments_for_existing_and_new_files() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nested")).unwrap();
        let path = directory.path().join("tasks.md");
        let alias = directory.path().join("nested").join("..").join("tasks.md");
        assert_eq!(
            canonical_path(&path, true).unwrap(),
            canonical_path(&alias, true).unwrap()
        );
        fs::write(&path, "tasks").unwrap();
        assert_eq!(
            canonical_path(&path, false).unwrap(),
            canonical_path(&alias, false).unwrap()
        );
    }
    #[test]
    fn config_roundtrips_and_corrupt_config_is_not_missing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state").join("config.json");
        assert!(load_json(&path).unwrap().is_none());
        let config = serde_json::json!({"theme":"glass", "darkMode":true});
        save_json(&path, &config).unwrap();
        assert_eq!(load_json(&path).unwrap(), Some(config));
        std::fs::write(&path, "invalid").unwrap();
        assert_eq!(load_json(&path).unwrap_err().code, "STATE_INVALID");
    }
}
