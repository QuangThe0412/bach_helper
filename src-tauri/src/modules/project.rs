use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectScanResult {
    pub project_path: String,
    pub is_valid_directory: bool,
    pub error_message: Option<String>,

    pub build_folder_found: bool,
    pub build_folder_path: Option<String>,

    pub build_xml_found: bool,
    pub build_xml_path: Option<String>,
    pub build_xml_location_note: Option<String>,

    pub filter_folder_found: bool,
    pub filter_folder_path: Option<String>,
    pub filter_files: Vec<String>,

    // Build.xml parsed preview
    pub project_name: Option<String>,
    pub default_target: Option<String>,
    pub ant_targets: Vec<String>,

    pub summary_messages: Vec<String>,
}

pub fn scan_project(path_str: &str) -> ProjectScanResult {
    let clean_path = path_str.trim().trim_matches('"');
    let base_path = Path::new(clean_path);

    if !base_path.exists() {
        return ProjectScanResult {
            project_path: clean_path.to_string(),
            is_valid_directory: false,
            error_message: Some(format!("Directory does not exist: {}", clean_path)),
            build_folder_found: false,
            build_folder_path: None,
            build_xml_found: false,
            build_xml_path: None,
            build_xml_location_note: None,
            filter_folder_found: false,
            filter_folder_path: None,
            filter_files: Vec::new(),
            project_name: None,
            default_target: None,
            ant_targets: Vec::new(),
            summary_messages: vec![format!("Error: Folder '{}' not found on system.", clean_path)],
        };
    }

    if !base_path.is_dir() {
        return ProjectScanResult {
            project_path: clean_path.to_string(),
            is_valid_directory: false,
            error_message: Some(format!("Path is not a directory: {}", clean_path)),
            build_folder_found: false,
            build_folder_path: None,
            build_xml_found: false,
            build_xml_path: None,
            build_xml_location_note: None,
            filter_folder_found: false,
            filter_folder_path: None,
            filter_files: Vec::new(),
            project_name: None,
            default_target: None,
            ant_targets: Vec::new(),
            summary_messages: vec![format!("Error: '{}' is not a folder.", clean_path)],
        };
    }

    let mut summary = Vec::new();
    summary.push(format!("Scanned base directory: {}", clean_path));

    // 1. Check folder 'build'
    let build_folder_candidate = base_path.join("build");
    let (build_folder_found, build_folder_path) = if build_folder_candidate.is_dir() {
        summary.push("Found 'build' directory.".to_string());
        (true, Some(build_folder_candidate.to_string_lossy().to_string()))
    } else {
        summary.push("Folder 'build' not found in project root.".to_string());
        (false, None)
    };

    // 2. Check file 'build.xml'
    // Primary: inside 'build' folder
    // Alternative: in project root
    let mut build_xml_found = false;
    let mut build_xml_path: Option<String> = None;
    let mut build_xml_location_note: Option<String> = None;
    let mut xml_file_path: Option<PathBuf> = None;

    if build_folder_found {
        let inside_build_xml = build_folder_candidate.join("build.xml");
        if inside_build_xml.is_file() {
            build_xml_found = true;
            build_xml_path = Some(inside_build_xml.to_string_lossy().to_string());
            build_xml_location_note = Some("Located inside 'build' directory (Standard)".to_string());
            xml_file_path = Some(inside_build_xml);
            summary.push("Found 'build.xml' inside 'build' folder.".to_string());
        }
    }

    if !build_xml_found {
        let root_build_xml = base_path.join("build.xml");
        if root_build_xml.is_file() {
            build_xml_found = true;
            build_xml_path = Some(root_build_xml.to_string_lossy().to_string());
            build_xml_location_note = Some("Located in project root directory".to_string());
            xml_file_path = Some(root_build_xml);
            summary.push("Found 'build.xml' in project root directory.".to_string());
        }
    }

    if !build_xml_found {
        summary.push("'build.xml' NOT found in 'build' folder or project root.".to_string());
    }

    // 3. Check folder 'filter' or 'filters'
    // Primary: inside 'build' folder
    // Alternative: in project root
    let mut filter_folder_found = false;
    let mut filter_folder_path: Option<String> = None;
    let mut filter_path_buf: Option<PathBuf> = None;

    if build_folder_found {
        let build_filter = build_folder_candidate.join("filter");
        let build_filters = build_folder_candidate.join("filters");
        if build_filter.is_dir() {
            filter_folder_found = true;
            filter_folder_path = Some(build_filter.to_string_lossy().to_string());
            filter_path_buf = Some(build_filter);
            summary.push("Found 'filter' directory inside 'build' folder.".to_string());
        } else if build_filters.is_dir() {
            filter_folder_found = true;
            filter_folder_path = Some(build_filters.to_string_lossy().to_string());
            filter_path_buf = Some(build_filters);
            summary.push("Found 'filters' directory inside 'build' folder.".to_string());
        }
    }

    if !filter_folder_found {
        let root_filter = base_path.join("filter");
        let root_filters = base_path.join("filters");
        if root_filter.is_dir() {
            filter_folder_found = true;
            filter_folder_path = Some(root_filter.to_string_lossy().to_string());
            filter_path_buf = Some(root_filter);
            summary.push("Found 'filter' directory in project root.".to_string());
        } else if root_filters.is_dir() {
            filter_folder_found = true;
            filter_folder_path = Some(root_filters.to_string_lossy().to_string());
            filter_path_buf = Some(root_filters);
            summary.push("Found 'filters' directory in project root.".to_string());
        }
    }

    if !filter_folder_found {
        summary.push("Folder 'filter' / 'filters' NOT found.".to_string());
    }

    // Scan files inside filter directory
    let mut filter_files = Vec::new();
    if let Some(f_dir) = filter_path_buf {
        if let Ok(entries) = fs::read_dir(f_dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        filter_files.push(name);
                    }
                }
            }
        }
        filter_files.sort();
        if !filter_files.is_empty() {
            summary.push(format!("Found {} filter file(s): {}", filter_files.len(), filter_files.join(", ")));
        }
    }

    // 4. Parse build.xml for targets if present
    let mut project_name = None;
    let mut default_target = None;
    let mut ant_targets = Vec::new();

    if let Some(xml_path) = xml_file_path {
        if let Ok(content) = fs::read_to_string(xml_path) {
            // Simple robust regex-less or line-based XML parser for project and targets
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.contains("<project") {
                    if let Some(name) = extract_attribute(trimmed, "name") {
                        project_name = Some(name);
                    }
                    if let Some(def) = extract_attribute(trimmed, "default") {
                        default_target = Some(def);
                    }
                }
                if trimmed.contains("<target") {
                    if let Some(target_name) = extract_attribute(trimmed, "name") {
                        if !ant_targets.contains(&target_name) {
                            ant_targets.push(target_name);
                        }
                    }
                }
            }
        }
        if let Some(ref p_name) = project_name {
            summary.push(format!("Ant Project Name: {}", p_name));
        }
        if let Some(ref d_tgt) = default_target {
            summary.push(format!("Ant Default Target: {}", d_tgt));
        }
        if !ant_targets.is_empty() {
            summary.push(format!("Discovered {} target(s): {}", ant_targets.len(), ant_targets.join(", ")));
        }
    }

    ProjectScanResult {
        project_path: clean_path.to_string(),
        is_valid_directory: true,
        error_message: None,
        build_folder_found,
        build_folder_path,
        build_xml_found,
        build_xml_path,
        build_xml_location_note,
        filter_folder_found,
        filter_folder_path,
        filter_files,
        project_name,
        default_target,
        ant_targets,
        summary_messages: summary,
    }
}

fn extract_attribute(line: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    if let Some(start_idx) = line.find(&pattern) {
        let value_start = start_idx + pattern.len();
        if let Some(end_idx) = line[value_start..].find('"') {
            return Some(line[value_start..value_start + end_idx].to_string());
        }
    }
    let pattern_sq = format!("{}='", attr);
    if let Some(start_idx) = line.find(&pattern_sq) {
        let value_start = start_idx + pattern_sq.len();
        if let Some(end_idx) = line[value_start..].find('\'') {
            return Some(line[value_start..value_start + end_idx].to_string());
        }
    }
    None
}

