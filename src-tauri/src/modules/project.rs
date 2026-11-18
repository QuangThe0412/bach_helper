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
            let (p_name, d_tgt, tgts) = parse_build_xml(&content);
            project_name = p_name;
            default_target = d_tgt;
            ant_targets = tgts;
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

/// Robust XML tag parser to extract project name, default target, and build targets
pub fn parse_build_xml(content: &str) -> (Option<String>, Option<String>, Vec<String>) {
    let mut project_name = None;
    let mut default_target = None;
    let mut targets = Vec::new();

    // 1. Find <project ...>
    if let Some(proj_start) = content.find("<project") {
        if let Some(proj_end) = content[proj_start..].find('>') {
            let tag_content = &content[proj_start..proj_start + proj_end];
            project_name = extract_attr_from_tag(tag_content, "name");
            default_target = extract_attr_from_tag(tag_content, "default");
        }
    }

    // 2. Find all <target ...>
    let mut search_from = 0;
    while let Some(rel_start) = content[search_from..].find("<target") {
        let tag_start = search_from + rel_start;
        if let Some(rel_end) = content[tag_start..].find('>') {
            let tag_end = tag_start + rel_end;
            let tag_content = &content[tag_start..tag_end];

            // Extract the 'name' attribute which defines the target to build
            if let Some(target_name) = extract_attr_from_tag(tag_content, "name") {
                let trimmed_name = target_name.trim().to_string();
                if !trimmed_name.is_empty() && !targets.contains(&trimmed_name) {
                    targets.push(trimmed_name);
                }
            }
            search_from = tag_end + 1;
        } else {
            break;
        }
    }

    // Filter: if there are package targets, exclude internal helper targets like "compile", "init", "clean"
    let has_packages = targets.iter().any(|t| t.contains("package"));
    if has_packages {
        targets.retain(|t| t != "compile" && t != "init" && t != "clean");
    }

    (project_name, default_target, targets)
}

fn extract_attr_from_tag(tag: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pattern = format!("{}=\\", attr).replace('\\', &quote.to_string());
        if let Some(start_idx) = tag.find(&pattern) {
            let val_start = start_idx + pattern.len();
            if let Some(end_idx) = tag[val_start..].find(quote) {
                return Some(tag[val_start..val_start + end_idx].to_string());
            }
        }
    }
    None
}

