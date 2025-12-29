use std::fs;
use std::path::PathBuf;

/// Embedded default syntax files
const SYNTAX_RS: &str = include_str!("../defaults/syntax/rs.ue-syntax");
const SYNTAX_PY: &str = include_str!("../defaults/syntax/py.ue-syntax");
const SYNTAX_JS: &str = include_str!("../defaults/syntax/js.ue-syntax");
const SYNTAX_TS: &str = include_str!("../defaults/syntax/ts.ue-syntax");
const SYNTAX_C: &str = include_str!("../defaults/syntax/c.ue-syntax");
const SYNTAX_CPP: &str = include_str!("../defaults/syntax/cpp.ue-syntax");
const SYNTAX_GO: &str = include_str!("../defaults/syntax/go.ue-syntax");
const SYNTAX_JAVA: &str = include_str!("../defaults/syntax/java.ue-syntax");
const SYNTAX_SH: &str = include_str!("../defaults/syntax/sh.ue-syntax");
const SYNTAX_HTML: &str = include_str!("../defaults/syntax/html.ue-syntax");
const SYNTAX_CSS: &str = include_str!("../defaults/syntax/css.ue-syntax");
const SYNTAX_MD: &str = include_str!("../defaults/syntax/md.ue-syntax");
const SYNTAX_JSON: &str = include_str!("../defaults/syntax/json.ue-syntax");
const SYNTAX_XML: &str = include_str!("../defaults/syntax/xml.ue-syntax");
const SYNTAX_TOML: &str = include_str!("../defaults/syntax/toml.ue-syntax");
const SYNTAX_YAML: &str = include_str!("../defaults/syntax/yaml.ue-syntax");
const SYNTAX_SQL: &str = include_str!("../defaults/syntax/sql.ue-syntax");
const SYNTAX_TXT: &str = include_str!("../defaults/syntax/txt.ue-syntax");
const SYNTAX_UE_SYNTAX: &str = include_str!("../defaults/syntax/ue-syntax.ue-syntax");

/// Get embedded default syntax content for a given extension
fn get_default_syntax(extension: &str) -> Option<&'static str> {
    match extension {
        "rs" => Some(SYNTAX_RS),
        "py" => Some(SYNTAX_PY),
        "js" => Some(SYNTAX_JS),
        "ts" => Some(SYNTAX_TS),
        "c" => Some(SYNTAX_C),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(SYNTAX_CPP),
        "h" => Some(SYNTAX_C), // .h files default to C (user can override with cpp version)
        "go" => Some(SYNTAX_GO),
        "java" => Some(SYNTAX_JAVA),
        "sh" | "bash" | "zsh" => Some(SYNTAX_SH),
        "html" | "htm" => Some(SYNTAX_HTML),
        "css" => Some(SYNTAX_CSS),
        "md" | "markdown" => Some(SYNTAX_MD),
        "json" => Some(SYNTAX_JSON),
        "xml" => Some(SYNTAX_XML),
        "toml" => Some(SYNTAX_TOML),
        "yaml" | "yml" => Some(SYNTAX_YAML),
        "sql" => Some(SYNTAX_SQL),
        "txt" => Some(SYNTAX_TXT),
        "ue-syntax" => Some(SYNTAX_UE_SYNTAX),
        _ => None,
    }
}

/// Deploy all default syntax files to ~/.ue/syntax/, skipping existing files
#[allow(dead_code)]
pub fn deploy_default_syntax_files() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))?;
    let syntax_dir = PathBuf::from(home).join(".ue").join("syntax");

    // Create directory if it doesn't exist
    fs::create_dir_all(&syntax_dir)?;

    // List of all syntax files to deploy (extension, content)
    let syntax_files = [
        ("rs", SYNTAX_RS),
        ("py", SYNTAX_PY),
        ("js", SYNTAX_JS),
        ("ts", SYNTAX_TS),
        ("c", SYNTAX_C),
        ("cpp", SYNTAX_CPP),
        ("go", SYNTAX_GO),
        ("java", SYNTAX_JAVA),
        ("sh", SYNTAX_SH),
        ("html", SYNTAX_HTML),
        ("css", SYNTAX_CSS),
        ("md", SYNTAX_MD),
        ("json", SYNTAX_JSON),
        ("xml", SYNTAX_XML),
        ("toml", SYNTAX_TOML),
        ("yaml", SYNTAX_YAML),
        ("yml", SYNTAX_YAML), // yml uses same as yaml
        ("sql", SYNTAX_SQL),
        ("txt", SYNTAX_TXT),
        ("ue-syntax", SYNTAX_UE_SYNTAX),
    ];

    // Deploy each file if it doesn't exist
    for (ext, content) in syntax_files {
        let file_path = syntax_dir.join(format!("{}.ue-syntax", ext));

        // Skip if file already exists (don't overwrite user customizations)
        if !file_path.exists() {
            fs::write(&file_path, content)?;
        }
    }

    Ok(())
}

/// Get syntax file content for a given extension.
/// First checks ~/.ue/syntax/, then falls back to embedded defaults.
pub(crate) fn get_syntax_content(extension: &str) -> Option<String> {
    // Try user file first
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let user_path = PathBuf::from(home)
            .join(".ue")
            .join("syntax")
            .join(format!("{}.ue-syntax", extension));

        if let Ok(content) = fs::read_to_string(&user_path) {
            return Some(content);
        }
    }

    // Fall back to embedded default
    get_default_syntax(extension).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_syntax_exists() {
        assert!(get_default_syntax("rs").is_some());
        assert!(get_default_syntax("py").is_some());
        assert!(get_default_syntax("js").is_some());
        assert!(get_default_syntax("ue-syntax").is_some());
        assert!(get_default_syntax("unknown_ext").is_none());
    }

    #[test]
    fn test_cpp_aliases() {
        assert!(get_default_syntax("cpp").is_some());
        assert!(get_default_syntax("hpp").is_some());
        assert!(get_default_syntax("h").is_some());
    }
}
