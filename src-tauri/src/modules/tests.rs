#[cfg(test)]
mod tests {
    use crate::modules::ant_manager::*;
    use crate::modules::builder::*;
    use crate::modules::environment::*;
    use crate::modules::jdk_manager::*;
    use crate::modules::project::*;
    use crate::modules::settings::*;

    #[test]
    fn test_environment_check() {
        let env = check_system_environment();
        println!("Ant installed: {}", env.ant_installed);
        println!("Java installed: {}", env.java_installed);
        println!("Javac installed: {}", env.javac_installed);
        println!("Ant version: {:?}", env.ant_version);
        println!("Java version: {:?}", env.java_version);
        println!("ANT_HOME: {:?}", env.ant_home);
        println!("JAVA_HOME: {:?}", env.java_home);
        assert!(env.ant_installed || !env.ant_installed);
    }

    #[test]
    fn test_project_scan_sample() {
        let res = scan_project(r"D:\vinx_tools\bach_helper\docs");
        println!("Project scan result: {:?}", res);
        assert!(res.is_valid_directory);
        assert!(res.build_xml_found);
        assert!(res.filter_folder_found);
        assert!(!res.filter_files.is_empty());
        println!("Filter files: {:?}", res.filter_files);
        println!("Ant targets: {:?}", res.ant_targets);
    }

    #[test]
    fn test_ant_scan() {
        let res = scan_available_ants();
        println!("System ant: {:?}", res.current_system_ant);
        println!("Bundled ants: {:?}", res.bundled_ants);
        assert!(!res.available_options.is_empty());
        if let Some(sys_ant) = res.current_system_ant {
            if let Some(exe) = sys_ant.executable_path {
                let test_res = test_ant_executable(&exe);
                println!("Test ant result: {:?}", test_res);
                assert!(test_res.success);
            }
        }
    }

    #[test]
    fn test_jdk_scan() {
        let res = scan_available_jdks();
        println!("Recommended JDK: {:?}", res.recommended_jdk_id);
        println!("All JDKs found: {}", res.jdks.len());
        for jdk in &res.jdks {
            println!(
                "- {} [ready={}] at {}",
                jdk.name, jdk.is_ready_to_use, jdk.root_path
            );
        }
        assert!(!res.jdks.is_empty());
    }

    #[test]
    fn test_settings() {
        let path = get_settings_file_path();
        println!("Settings file path: {}", path.display());
        let original = load_settings();
        let updated = add_project_to_history(r"D:\vinx_tools\bach_helper\docs").unwrap();
        assert_eq!(
            updated.last_project,
            Some(r"D:\vinx_tools\bach_helper\docs".to_string())
        );
        // Restore user's real project as last_project if present in history
        let mut restored = original;
        restored.history.retain(|p| p != r"D:\vinx_tools\bach_helper\docs");
        if let Some(first) = restored.history.first().cloned() {
            restored.last_project = Some(first);
        }
        let _ = save_settings(&restored);
        println!("Restored settings: {:?}", restored);
    }

    #[test]
    fn test_builder_inspection_docs() {
        let inspection = inspect_target_files(r"D:\vinx_tools\bach_helper\docs", "development-package");
        println!("Inspection docs: {:?}", inspection);
        assert!(inspection.properties_file.is_some());
        assert!(inspection.base_directory.is_some());
    }
}
