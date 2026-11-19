use crate::modules::builder::{encode_utf16le_base64, find_build_dir, find_properties_for_target};
use crate::modules::encoding::{detect_and_decode_text, unescape_properties_value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfigInfo {
    pub found: bool,
    pub properties_file: Option<String>,
    pub db_filter_file: Option<String>,
    pub db_type: String,
    pub driver: Option<String>,
    pub database_name_alias: Option<String>,
    pub host: Option<String>,
    pub port_or_instance: Option<String>,
    pub effective_server: Option<String>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub init_pool: Option<String>,
    pub max_pool: Option<String>,
    pub jdbc_url: Option<String>,
    pub summary_label: String,
    pub warning_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseTestResult {
    pub success: bool,
    pub db_type: String,
    pub target_server: String,
    pub database: String,
    pub user: String,
    pub duration_ms: u64,
    pub server_version: Option<String>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct PsSqlTestOutput {
    success: bool,
    server_version: Option<String>,
    message: String,
}

/// Unescape Java `.properties` escape sequences (`\uXXXX`, `\\`, `\:`, `\=`).
pub fn unescape_java_property_value(raw: &str) -> String {
    let unicode_decoded = unescape_properties_value(raw);
    let chars: Vec<char> = unicode_decoded.chars().collect();
    let mut out = String::with_capacity(unicode_decoded.len());
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                '\\' => {
                    out.push('\\');
                    i += 2;
                    continue;
                }
                ':' => {
                    out.push(':');
                    i += 2;
                    continue;
                }
                '=' => {
                    out.push('=');
                    i += 2;
                    continue;
                }
                'n' => {
                    out.push('\n');
                    i += 2;
                    continue;
                }
                'r' => {
                    out.push('\r');
                    i += 2;
                    continue;
                }
                't' => {
                    out.push('\t');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

fn parse_properties_map(prop_file: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(bytes) = fs::read(prop_file) else {
        return map;
    };
    let (content, _) = detect_and_decode_text(&bytes);

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some(eq_idx) = trimmed.find('=') {
            let key = trimmed[..eq_idx].trim();
            let val = trimmed[eq_idx + 1..].trim();
            if !key.is_empty() {
                map.insert(key.to_string(), unescape_java_property_value(val));
            }
        }
    }

    map
}

fn find_database_filter_file(project_path: &str) -> Option<PathBuf> {
    let clean_proj = project_path.trim().trim_matches('"').replace('/', "\\");
    if clean_proj.is_empty() {
        return None;
    }
    let base_proj = Path::new(&clean_proj);

    let direct_candidates = [
        base_proj
            .join("batch")
            .join("properties")
            .join("database.properties.filter"),
        base_proj
            .join("batch")
            .join("properties")
            .join("database.properties"),
        base_proj
            .join("target")
            .join("batch")
            .join("properties")
            .join("database.properties"),
        base_proj.join("database.properties.filter"),
        base_proj.join("database.properties"),
    ];

    for c in direct_candidates {
        if c.is_file() {
            return Some(c);
        }
    }

    for root_dir in [base_proj.join("batch"), base_proj.join("target")] {
        if root_dir.is_dir() {
            for entry in walkdir::WalkDir::new(&root_dir)
                .max_depth(4)
                .into_iter()
                .flatten()
            {
                if entry.file_type().is_file() {
                    let fname_lower = entry.file_name().to_string_lossy().to_lowercase();
                    if fname_lower == "database.properties.filter"
                        || fname_lower == "database.properties"
                    {
                        return Some(entry.path().to_path_buf());
                    }
                }
            }
        }
    }

    None
}

fn parse_database_filter_entries(filter_file: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(bytes) = fs::read(filter_file) else {
        return map;
    };
    let (content, _) = detect_and_decode_text(&bytes);

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some(comma_idx) = trimmed.find(',') {
            let key = trimmed[..comma_idx].trim().to_lowercase();
            let val = trimmed[comma_idx + 1..].trim().to_string();
            if !key.is_empty() {
                map.insert(key, val);
            }
        } else if let Some(eq_idx) = trimmed.find('=') {
            let key = trimmed[..eq_idx].trim().to_lowercase();
            let val = trimmed[eq_idx + 1..].trim().to_string();
            if !key.is_empty() {
                map.insert(key, val);
            }
        }
    }

    map
}

fn substitute_ant_tokens(template: &str, prop_map: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (k, v) in prop_map {
        let token = format!("@{}@", k);
        if result.contains(&token) {
            result = result.replace(&token, v);
        }
    }
    result
}

fn has_unresolved_ant_token(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            if let Some(end_rel) = s[i + 1..].find('@') {
                let inner = &s[i + 1..i + 1 + end_rel];
                if !inner.is_empty() && !inner.contains(' ') && !inner.contains(';') {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn first_non_empty<'a>(prop_map: &'a HashMap<String, String>, keys: &[&str]) -> Option<String> {
    for &k in keys {
        if let Some(v) = prop_map.get(k) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn detect_db_type(driver: Option<&str>, jdbc_url: Option<&str>, host: Option<&str>) -> String {
    let combined = format!(
        "{} {} {}",
        driver.unwrap_or(""),
        jdbc_url.unwrap_or(""),
        host.unwrap_or("")
    )
    .to_lowercase();

    if combined.contains("sqlserver") || combined.contains("microsoft") {
        "SQL Server".to_string()
    } else if combined.contains("oracle") {
        "Oracle".to_string()
    } else if combined.contains("postgresql") || combined.contains("postgres") {
        "PostgreSQL".to_string()
    } else if combined.contains("mysql") || combined.contains("mariadb") {
        "MySQL".to_string()
    } else if !combined.trim().is_empty() {
        "Database".to_string()
    } else {
        "Unknown".to_string()
    }
}

fn extract_database_from_jdbc_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if let Some(idx) = lower.find("databasename=") {
        let after = &url[idx + "databasename=".len()..];
        let val = after.split(';').next().unwrap_or("").trim();
        if !val.is_empty() && !has_unresolved_ant_token(val) {
            return Some(val.to_string());
        }
    }
    None
}

fn extract_sqlserver_from_jdbc_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if let Some(idx) = lower.find("jdbc:sqlserver://") {
        let after = &url[idx + "jdbc:sqlserver://".len()..];
        let seg = after.split(';').next().unwrap_or("").trim();
        if !seg.is_empty() && !has_unresolved_ant_token(seg) {
            return Some(seg.to_string());
        }
    }
    None
}

fn compute_effective_server(
    host: Option<&str>,
    port_or_instance: Option<&str>,
    jdbc_url: Option<&str>,
    db_type: &str,
) -> Option<String> {
    if let Some(url) = jdbc_url {
        if let Some(srv) = extract_sqlserver_from_jdbc_url(url) {
            return Some(srv);
        }
    }

    let h = host?.trim();
    if h.is_empty() {
        return None;
    }

    if h.contains('\\') || h.contains(':') || h.contains(',') {
        return Some(h.to_string());
    }

    if let Some(p) = port_or_instance.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if p.chars().all(|c| c.is_ascii_digit()) {
            if db_type == "SQL Server" {
                return Some(format!("{},{}", h, p));
            }
            return Some(format!("{}:{}", h, p));
        }
        return Some(format!("{}\\{}", h, p));
    }

    Some(h.to_string())
}

/// Inspect database settings from the active project + target `.properties` file and `database.properties.filter`.
pub fn inspect_database_config(project_path: &str, target: &str) -> DatabaseConfigInfo {
    let clean_proj = project_path.trim().trim_matches('"');
    if clean_proj.is_empty() {
        return DatabaseConfigInfo {
            found: false,
            properties_file: None,
            db_filter_file: None,
            db_type: "Unknown".to_string(),
            driver: None,
            database_name_alias: None,
            host: None,
            port_or_instance: None,
            effective_server: None,
            database: None,
            user: None,
            password: None,
            init_pool: None,
            max_pool: None,
            jdbc_url: None,
            summary_label: "N/A".to_string(),
            warning_message: None,
        };
    }

    let build_dir = find_build_dir(clean_proj);
    let prop_path = build_dir
        .as_ref()
        .and_then(|bd| find_properties_for_target(bd, target));
    let prop_map = prop_path
        .as_ref()
        .map(|p| parse_properties_map(p))
        .unwrap_or_default();

    let db_filter_path = find_database_filter_file(clean_proj);
    let filter_map = db_filter_path
        .as_ref()
        .map(|p| parse_database_filter_entries(p))
        .unwrap_or_default();

    let driver = filter_map.get("driver").cloned();
    let database_name_alias = filter_map.get("database_name").cloned();

    let raw_url = filter_map.get("url").map(|u| substitute_ant_tokens(u, &prop_map));
    let resolved_filter_user = filter_map
        .get("user")
        .map(|u| substitute_ant_tokens(u, &prop_map))
        .filter(|s| !s.is_empty() && !has_unresolved_ant_token(s));
    let resolved_filter_pswd = filter_map
        .get("pswd")
        .map(|p| substitute_ant_tokens(p, &prop_map))
        .filter(|s| !s.is_empty() && !has_unresolved_ant_token(s));
    let resolved_filter_init = filter_map
        .get("initarray")
        .map(|v| substitute_ant_tokens(v, &prop_map))
        .filter(|s| !s.is_empty() && !has_unresolved_ant_token(s));
    let resolved_filter_max = filter_map
        .get("maxarray")
        .map(|v| substitute_ant_tokens(v, &prop_map))
        .filter(|s| !s.is_empty() && !has_unresolved_ant_token(s));

    let host = first_non_empty(
        &prop_map,
        &[
            "pom.db.host.value",
            "db.host",
            "database.host",
            "pom.db.server.value",
        ],
    );
    let port_or_instance = first_non_empty(
        &prop_map,
        &[
            "pom.db.port.value",
            "db.port",
            "database.port",
            "pom.db.instance.value",
        ],
    );

    let url_db = raw_url.as_deref().and_then(extract_database_from_jdbc_url);
    let database = first_non_empty(
        &prop_map,
        &[
            "pom.db.database.value",
            "pom.db.name.value",
            "pom.db.serviceName.value",
            "pom.db.sid.value",
            "db.database",
            "db.name",
            "database.name",
        ],
    )
    .or(url_db);

    let user = first_non_empty(
        &prop_map,
        &["pom.db.user.value", "db.user", "db.username", "database.user"],
    )
    .or(resolved_filter_user);

    let password = first_non_empty(
        &prop_map,
        &[
            "pom.db.pswd.value",
            "pom.db.password.value",
            "db.pswd",
            "db.password",
            "database.password",
        ],
    )
    .or(resolved_filter_pswd);

    let init_pool = first_non_empty(&prop_map, &["pom.db.init.value", "db.init"])
        .or(resolved_filter_init);
    let max_pool = first_non_empty(&prop_map, &["pom.db.max.value", "db.max"])
        .or(resolved_filter_max);

    let db_type = detect_db_type(driver.as_deref(), raw_url.as_deref(), host.as_deref());
    let effective_server = compute_effective_server(
        host.as_deref(),
        port_or_instance.as_deref(),
        raw_url.as_deref(),
        &db_type,
    );

    let warning_message = raw_url.as_ref().and_then(|u| {
        if has_unresolved_ant_token(u) {
            Some(format!(
                "JDBC URL template contains unresolved placeholder(s): {}",
                u
            ))
        } else {
            None
        }
    });

    let found = host.is_some()
        || database.is_some()
        || user.is_some()
        || effective_server.is_some()
        || raw_url.is_some();

    let summary_label = if !found {
        "N/A".to_string()
    } else {
        match (database.as_deref(), effective_server.as_deref()) {
            (Some(db), Some(srv)) => format!("{} ({})", db, srv),
            (Some(db), None) => db.to_string(),
            (None, Some(srv)) => srv.to_string(),
            (None, None) => "Configured".to_string(),
        }
    };

    DatabaseConfigInfo {
        found,
        properties_file: prop_path.map(|p| p.to_string_lossy().to_string()),
        db_filter_file: db_filter_path.map(|p| p.to_string_lossy().to_string()),
        db_type,
        driver,
        database_name_alias,
        host,
        port_or_instance,
        effective_server,
        database,
        user,
        password,
        init_pool,
        max_pool,
        jdbc_url: raw_url,
        summary_label,
        warning_message,
    }
}

#[cfg(windows)]
fn normalize_sqlserver_datasource(server: &str) -> String {
    let trimmed = server.trim();
    if let Some(colon_idx) = trimmed.rfind(':') {
        let port_part = &trimmed[colon_idx + 1..];
        if port_part.chars().all(|c| c.is_ascii_digit()) {
            return format!("{},{}", &trimmed[..colon_idx], port_part);
        }
    }
    trimmed.to_string()
}

#[cfg(windows)]
fn ps_single_quote_escape(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(windows)]
fn test_sqlserver_via_dotnet(
    server: &str,
    database: Option<&str>,
    user: Option<&str>,
    password: Option<&str>,
) -> Result<(bool, Option<String>, String), String> {
    let ds = ps_single_quote_escape(&normalize_sqlserver_datasource(server));
    let db = ps_single_quote_escape(database.unwrap_or(""));
    let uid = ps_single_quote_escape(user.unwrap_or(""));
    let pwd = ps_single_quote_escape(password.unwrap_or(""));

    let ps_script = format!(
        r#"$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Data
$out = @{{ success = $false; server_version = $null; message = '' }}
try {{
    $csb = New-Object System.Data.SqlClient.SqlConnectionStringBuilder
    $csb['Data Source'] = '{ds}'
    if ('{db}' -ne '') {{ $csb['Initial Catalog'] = '{db}' }}
    if ('{uid}' -ne '') {{
        $csb['Integrated Security'] = $false
        $csb['User ID'] = '{uid}'
        $csb['Password'] = '{pwd}'
    }} else {{
        $csb['Integrated Security'] = $true
    }}
    $csb['Connect Timeout'] = 5
    $csb['Encrypt'] = $false
    $csb['TrustServerCertificate'] = $true

    $conn = New-Object System.Data.SqlClient.SqlConnection($csb.ConnectionString)
    $conn.Open()
    $ver = $conn.ServerVersion
    try {{
        $cmd = $conn.CreateCommand()
        $cmd.CommandTimeout = 4
        $cmd.CommandText = 'SELECT TOP 1 @@VERSION'
        $rawVer = $cmd.ExecuteScalar()
        if ($null -ne $rawVer) {{
            $firstLine = ($rawVer.ToString() -split "`r?`n")[0].Trim()
            if ($firstLine -ne '') {{ $ver = $firstLine }}
        }}
        $cmd.Dispose()
    }} catch {{}}
    $conn.Close()
    $conn.Dispose()
    $out.success = $true
    $out.server_version = $ver
    $out.message = 'SQL Server connection succeeded.'
}} catch {{
    $errMsg = $_.Exception.Message
    if ($_.Exception.InnerException -and $_.Exception.InnerException.Message) {{
        $errMsg = $_.Exception.InnerException.Message
    }}
    $out.success = $false
    $out.message = [string]$errMsg
}}
$json = $out | ConvertTo-Json -Compress
$sb = New-Object System.Text.StringBuilder
foreach ($ch in $json.ToCharArray()) {{
    $code = [int][char]$ch
    if ($code -le 127) {{
        [void]$sb.Append($ch)
    }} else {{
        [void]$sb.AppendFormat('\u{{0:x4}}', $code)
    }}
}}
Write-Output $sb.ToString()
"#,
        ds = ds,
        db = db,
        uid = uid,
        pwd = pwd
    );

    let encoded = encode_utf16le_base64(&ps_script);
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Failed to invoke PowerShell DB check: {}", e))?;

    let stdout_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let json_line = stdout_str
        .lines()
        .rev()
        .find(|l| l.trim().starts_with('{'))
        .map(|s| s.trim());

    if let Some(raw_json) = json_line {
        if let Ok(parsed) = serde_json::from_str::<PsSqlTestOutput>(raw_json) {
            return Ok((parsed.success, parsed.server_version, parsed.message));
        }
    }

    let stderr_str = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if !stderr_str.is_empty() {
        stderr_str
    } else if !stdout_str.is_empty() {
        stdout_str
    } else {
        "Unexpected empty response from database connection check.".to_string()
    })
}

fn default_port_for_db_type(db_type: &str) -> u16 {
    match db_type {
        "SQL Server" => 1433,
        "Oracle" => 1521,
        "PostgreSQL" => 5432,
        "MySQL" => 3306,
        _ => 1433,
    }
}

fn test_tcp_fallback(server: &str, db_type: &str) -> (bool, String) {
    let trimmed = server.trim();
    let (host_part, port) = if let Some(idx) = trimmed.rfind(':').or_else(|| trimmed.rfind(',')) {
        let p_str = trimmed[idx + 1..].trim();
        let port_num = p_str
            .parse::<u16>()
            .unwrap_or_else(|_| default_port_for_db_type(db_type));
        (trimmed[..idx].trim(), port_num)
    } else if let Some(bs_idx) = trimmed.find('\\') {
        (trimmed[..bs_idx].trim(), default_port_for_db_type(db_type))
    } else {
        (trimmed, default_port_for_db_type(db_type))
    };

    if host_part.is_empty() {
        return (false, "Database host is empty.".to_string());
    }

    let addr_str = format!("{}:{}", host_part, port);
    match addr_str.to_socket_addrs() {
        Ok(mut addrs) => {
            if let Some(sock_addr) = addrs.next() {
                match TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5)) {
                    Ok(_) => (
                        true,
                        format!("TCP reachability verified at {}:{} ({}).", host_part, port, db_type),
                    ),
                    Err(e) => (
                        false,
                        format!("TCP connection failed to {}:{} - {}", host_part, port, e),
                    ),
                }
            } else {
                (false, format!("Could not resolve host address: {}", host_part))
            }
        }
        Err(e) => (
            false,
            format!("Failed to resolve host '{}': {}", host_part, e),
        ),
    }
}

/// Test connection to the database configured in the selected project + target `.properties`.
pub fn test_database_connection(project_path: &str, target: &str) -> DatabaseTestResult {
    let started = Instant::now();
    let info = inspect_database_config(project_path, target);

    let server_str = info
        .effective_server
        .clone()
        .or_else(|| info.host.clone())
        .unwrap_or_default();
    let db_str = info.database.clone().unwrap_or_default();
    let user_str = info.user.clone().unwrap_or_default();

    if !info.found || server_str.trim().is_empty() {
        return DatabaseTestResult {
            success: false,
            db_type: info.db_type,
            target_server: server_str,
            database: db_str,
            user: user_str,
            duration_ms: started.elapsed().as_millis() as u64,
            server_version: None,
            message: "No database server configuration found in selected target .properties file."
                .to_string(),
        };
    }

    #[cfg(windows)]
    if info.db_type == "SQL Server" {
        match test_sqlserver_via_dotnet(
            &server_str,
            info.database.as_deref(),
            info.user.as_deref(),
            info.password.as_deref(),
        ) {
            Ok((success, server_version, message)) => {
                return DatabaseTestResult {
                    success,
                    db_type: info.db_type,
                    target_server: server_str,
                    database: db_str,
                    user: user_str,
                    duration_ms: started.elapsed().as_millis() as u64,
                    server_version,
                    message,
                };
            }
            Err(err_msg) => {
                return DatabaseTestResult {
                    success: false,
                    db_type: info.db_type,
                    target_server: server_str,
                    database: db_str,
                    user: user_str,
                    duration_ms: started.elapsed().as_millis() as u64,
                    server_version: None,
                    message: err_msg,
                };
            }
        }
    }

    let (success, message) = test_tcp_fallback(&server_str, &info.db_type);
    DatabaseTestResult {
        success,
        db_type: info.db_type,
        target_server: server_str,
        database: db_str,
        user: user_str,
        duration_ms: started.elapsed().as_millis() as u64,
        server_version: None,
        message,
    }
}
