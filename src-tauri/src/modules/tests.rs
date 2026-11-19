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

    #[test]
    fn test_user_setting_file() {
        use crate::modules::app_config::*;

        let path = get_setting_file_path();
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("setting.json"));

        let existed_before = path.exists();
        let original = load_user_setting();
        assert!(path.exists());

        let custom_dir = std::env::temp_dir().join(format!("bh_img_dir_{}", std::process::id()));
        let custom = UserSetting {
            ps_window_width: 1280,
            ps_window_height: 720,
            auto_capture_clipboard: false,
            auto_save_image: true,
            screenshot_save_dir: Some(custom_dir.to_string_lossy().to_string()),
        };
        let saved = save_user_setting(&custom).unwrap();
        assert_eq!(saved, custom);
        assert_eq!(load_user_setting(), custom);

        let clamped = save_user_setting(&UserSetting {
            ps_window_width: 50,
            ps_window_height: 99999,
            auto_capture_clipboard: true,
            auto_save_image: false,
            screenshot_save_dir: None,
        })
        .unwrap();
        assert_eq!(clamped.ps_window_width, MIN_PS_WIDTH);
        assert_eq!(clamped.ps_window_height, MAX_PS_HEIGHT);
        assert!(!clamped.auto_save_image);

        let _ = std::fs::remove_dir_all(&custom_dir);

        if existed_before {
            let _ = save_user_setting(&original);
        } else {
            let _ = save_user_setting(&UserSetting::default());
        }
    }

    #[test]
    fn test_scan_bat_files_strict_target() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("batch_helper_strict_test_{}", unique));
        let src_batfiles = root.join("batch").join("batfiles");
        std::fs::create_dir_all(&src_batfiles).unwrap();
        std::fs::write(src_batfiles.join("src.bat"), "@echo off\r\n").unwrap();

        // No target folder exists yet: MUST return empty and NOT fall back to source batch/batfiles
        let proj_str = root.to_string_lossy().to_string();
        let fake_base = root.join("target").to_string_lossy().to_string();
        let (dir_none, files_none) = scan_bat_files(&proj_str, Some(&fake_base));
        assert!(dir_none.is_none());
        assert!(files_none.is_empty());

        // Also if base_dir accidentally points to project root, never scan source batch/batfiles
        let (dir_proj, files_proj) = scan_bat_files(&proj_str, Some(&proj_str));
        assert!(dir_proj.is_none());
        assert!(files_proj.is_empty());

        // Now create target\batch\batfiles\out.bat -> MUST find ONLY out.bat from target
        let target_batfiles = root.join("target").join("batch").join("batfiles");
        std::fs::create_dir_all(&target_batfiles).unwrap();
        std::fs::write(target_batfiles.join("out.bat"), "@echo off\r\n").unwrap();

        let (dir_found, files_found) = scan_bat_files(&proj_str, Some(&fake_base));
        assert_eq!(
            dir_found.as_ref().map(|p| p.to_string_lossy().to_lowercase()),
            Some(target_batfiles.to_string_lossy().to_lowercase())
        );
        assert_eq!(files_found.len(), 1);
        assert_eq!(files_found[0].name, "out.bat");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_powershell_script_generation() {
        use crate::modules::app_config::UserSetting;

        let setting = UserSetting {
            ps_window_width: 1120,
            ps_window_height: 720,
            auto_capture_clipboard: true,
            auto_save_image: false,
            screenshot_save_dir: None,
        };
        let script = build_powershell_run_script(
            r"C:\proj\target\batch\batfiles",
            "demo.bat",
            &setting,
            None,
            None,
        );

        assert!(script.contains("Write-Host ('PS ' + (Get-Location).Path + '> .\\demo.bat')"));
        assert!(script.contains("& '.\\demo.bat'"));
        assert!(script.contains("function global:prompt"));
        assert!(script.contains("MoveWindow"));
        assert!(script.contains("PrimaryScreen.WorkingArea"));
        assert!(script.contains("Clipboard"));
        assert!(script.contains("1120"));
        assert!(script.contains("720"));
        assert!(!script.contains("Running:"));
        assert!(!script.contains("cmd.exe /c"));

        let no_cap_setting = UserSetting {
            ps_window_width: 1000,
            ps_window_height: 650,
            auto_capture_clipboard: false,
            auto_save_image: false,
            screenshot_save_dir: None,
        };
        let no_cap_script = build_powershell_run_script(
            r"C:\proj\target\batch\batfiles",
            "demo.bat",
            &no_cap_setting,
            None,
            None,
        );
        assert!(no_cap_script.contains("Write-Host ('PS ' + (Get-Location).Path + '> .\\demo.bat')"));
        assert!(!no_cap_script.contains("function global:prompt"));

        let b64 = encode_utf16le_base64(&script);
        assert!(!b64.is_empty());
        assert!(b64.is_ascii());
    }

    #[test]
    fn test_to_batch_safe_path() {
        let clean = to_batch_safe_path(r"D:\jdk-25.0.2\bin\javac");
        assert_eq!(clean, r"D:\jdk-25.0.2\bin\javac");

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("batch helper space {}", unique));
        std::fs::create_dir_all(&root).unwrap();

        let safe = to_batch_safe_path(&root.to_string_lossy());
        println!("to_batch_safe_path({}) -> {}", root.display(), safe);
        assert!(std::path::Path::new(&safe).exists());
        assert!(!safe.contains(' '));
        assert!(safe.is_ascii());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_remove_project_from_history() {
        let original = load_settings();
        let dummy = r"C:\__batch_helper_dummy_remove_test__";
        let added = add_project_to_history(dummy).unwrap();
        assert!(added.history.iter().any(|p| p.eq_ignore_ascii_case(dummy)));

        let removed = remove_project_from_history(dummy).unwrap();
        assert!(!removed.history.iter().any(|p| p.eq_ignore_ascii_case(dummy)));

        let _ = save_settings(&original);
    }

    #[test]
    fn test_run_auto_config_temp_project() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_autoconfig_test_{}", unique));
        let proj = base.join("proj with spaces");
        let build_dir = proj.join("build");
        let filters_dir = build_dir.join("filters");
        let mock_jdk = base.join("mock_jdk");
        std::fs::create_dir_all(&filters_dir).unwrap();
        std::fs::create_dir_all(mock_jdk.join("bin")).unwrap();
        std::fs::write(mock_jdk.join("bin").join("java.exe"), b"").unwrap();
        std::fs::write(mock_jdk.join("bin").join("javac.exe"), b"").unwrap();

        let build_xml = r#"<project name="demo" default="development-package">
  <target name="compile">
    <javac executable="C:\nonexistent_jdk\bin\javac" />
  </target>
  <target name="development-package" depends="compile">
    <filter filtersfile="filters/development.properties" />
  </target>
</project>
"#;
        std::fs::write(build_dir.join("build.xml"), build_xml).unwrap();

        let props = "pom.log4j.path.value=D:/old/proj/target/log/\nfilter.shellscript.javaHome.value=C:/nonexistent_jdk\nfilter.shellscript.baseDirectory.value=D:/old/proj/target\n";
        let prop_file = filters_dir.join("development.properties");
        std::fs::write(&prop_file, props).unwrap();

        let res = run_auto_config(
            &proj.to_string_lossy(),
            "development-package",
            Some(&mock_jdk.to_string_lossy()),
        )
        .unwrap();

        assert!(res.success);
        assert!(res.build_xml_updated);
        assert!(res.properties_updated);
        assert!(!res.batch_safe_project_path.contains(' '));

        let updated_xml = std::fs::read_to_string(build_dir.join("build.xml")).unwrap();
        assert!(updated_xml.contains(&mock_jdk.to_string_lossy().to_string()));

        let updated_props = std::fs::read_to_string(&prop_file).unwrap();
        assert!(updated_props.contains(&format!("{}/target", res.batch_safe_project_path)));
        assert!(updated_props.contains(&format!("{}/target/log/", res.batch_safe_project_path)));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_resolve_target_logs_strict() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("bh_strict_logs_{}", unique));
        let root_log = root.join("log");
        let batch_log = root.join("batch").join("log");
        std::fs::create_dir_all(&root_log).unwrap();
        std::fs::create_dir_all(&batch_log).unwrap();
        std::fs::write(root_log.join("batchApp.log"), "old").unwrap();
        std::fs::write(root_log.join("stclibApp.log"), "old").unwrap();
        std::fs::write(batch_log.join("batchApp.log"), "old").unwrap();

        let proj_str = root.to_string_lossy().to_string();
        assert!(resolve_log_file(&proj_str, None, None, "batchApp.log").0.is_none());
        assert!(resolve_log_file(&proj_str, None, None, "stclibApp.log").0.is_none());

        let target_log = root.join("target").join("log");
        std::fs::create_dir_all(&target_log).unwrap();
        std::fs::write(target_log.join("batchApp.log"), "new").unwrap();
        std::fs::write(target_log.join("stclibApp.log"), "new").unwrap();

        let (found_batch, _) = resolve_log_file(&proj_str, None, None, "batchApp.log");
        let (found_app, _) = resolve_log_file(&proj_str, None, None, "stclibApp.log");
        assert!(found_batch.unwrap().to_lowercase().contains("\\target\\log\\batchapp.log"));
        assert!(found_app.unwrap().to_lowercase().contains("\\target\\log\\stclibapp.log"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_encoding_japanese_roundtrip_all_types() {
        use crate::modules::encoding::*;

        // "REM Title: Promotion Data Process" in Japanese using pure-ASCII escapes
        let jp_comment = "REM \u{30bf}\u{30a4}\u{30c8}\u{30eb}\u{ff1a}\u{8ca9}\u{4fc3}\u{5b9f}\u{7e3e}\u{30c7}\u{30fc}\u{30bf}\u{4f5c}\u{6210}\u{51e6}\u{7406}\r\n";

        // 1. Shift-JIS (CP932) roundtrip for .bat
        let sjis_bytes = encode_text(jp_comment, TextEncoding::ShiftJis).unwrap();
        assert!(std::str::from_utf8(&sjis_bytes).is_err());
        let (decoded_bat, enc_bat) = detect_and_decode_text(&sjis_bytes);
        assert_eq!(enc_bat, TextEncoding::ShiftJis);
        assert_eq!(decoded_bat, jp_comment);
        assert!(!decoded_bat.contains("????"));

        // 2. UTF-8 and UTF-8 BOM preservation for .xml / .java / .txt
        let jp_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- \u{65e5}\u{672c}\u{8a9e}\u{30b3}\u{30e1}\u{30f3}\u{30c8} -->\n";
        let utf8_bytes = encode_text(jp_xml, TextEncoding::Utf8).unwrap();
        let (dec_xml, enc_xml) = detect_and_decode_text(&utf8_bytes);
        assert_eq!(enc_xml, TextEncoding::Utf8);
        assert_eq!(dec_xml, jp_xml);

        let bom_bytes = encode_text(jp_comment, TextEncoding::Utf8Bom).unwrap();
        assert!(bom_bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        let (dec_bom, enc_bom) = detect_and_decode_text(&bom_bytes);
        assert_eq!(enc_bom, TextEncoding::Utf8Bom);
        assert_eq!(dec_bom, jp_comment);

        // 3. Java .properties Unicode escaping & unescaping (prevents ISO-8859-1 mojibake in Ant filtersfile)
        let jp_path = "C:/Users/demo/\u{682a}\u{5f0f}\u{4f1a}\u{793e}\u{30f4}\u{30a3}\u{30f3}\u{30af}\u{30b9}/target";
        let escaped = escape_properties_value(jp_path);
        assert!(escaped.is_ascii());
        assert!(!escaped.contains("????"));
        assert_eq!(unescape_properties_value(&escaped), jp_path);
    }

    #[test]
    fn test_auto_config_java_home_all_key_variants() {
        use crate::modules::encoding::*;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_javahome_variants_{}", unique));
        let proj = base.join("\u{65e5}\u{672c}\u{8a9e}\u{30d7}\u{30ed}\u{30b8}\u{30a7}\u{30af}\u{30c8}");
        let build_dir = proj.join("build");
        let filters_dir = build_dir.join("filters");
        let mock_jdk25 = base.join("jdk-25.0.2");
        std::fs::create_dir_all(&filters_dir).unwrap();
        std::fs::create_dir_all(mock_jdk25.join("bin")).unwrap();
        std::fs::write(mock_jdk25.join("bin").join("java.exe"), b"").unwrap();
        std::fs::write(mock_jdk25.join("bin").join("javac.exe"), b"").unwrap();

        let build_xml = "<?xml version=\"1.0\" encoding=\"Shift_JIS\"?>\r\n<project name=\"jp\" default=\"development-package\">\r\n  <!-- \u{65e5}\u{672c}\u{8a9e}\u{30b3}\u{30e1}\u{30f3}\u{30c8} -->\r\n  <target name=\"compile\"><javac executable=\"D:\\java11\\bin\\javac\" /></target>\r\n  <target name=\"development-package\" depends=\"compile\"><filter filtersfile=\"filters/development.properties\" /></target>\r\n</project>\r\n";
        let xml_bytes = encode_text(build_xml, TextEncoding::ShiftJis).unwrap();
        std::fs::write(build_dir.join("build.xml"), xml_bytes).unwrap();

        let prop_text = "# \u{958b}\u{767a}\u{74b0}\u{5883}\r\npom.log4j.path.value=D:/old/target/log/\r\nfilter.shellscript.baseDirectory.value=D:/old/target\r\nfilter.javahome.baseDirectory.value=D:/java11\r\nfilter.shellscript.javaHome.value=D:/java11\r\ncustom.label=\u{8ca9}\u{4fc3}\u{30d0}\u{30c3}\u{30c1}\r\n";
        let prop_bytes = encode_text(prop_text, TextEncoding::ShiftJis).unwrap();
        let prop_file = filters_dir.join("development.properties");
        std::fs::write(&prop_file, prop_bytes).unwrap();

        let jdk_str = mock_jdk25.to_string_lossy().to_string();
        let res = run_auto_config(&proj.to_string_lossy(), "development-package", Some(&jdk_str)).unwrap();
        assert!(res.success);
        assert!(res.build_xml_updated);
        assert!(res.properties_updated);

        let updated_prop_bytes = std::fs::read(&prop_file).unwrap();
        let (updated_prop_str, _) = detect_and_decode_text(&updated_prop_bytes);
        assert!(!updated_prop_str.contains("D:/java11"));
        assert!(!updated_prop_str.contains("????"));
        assert!(updated_prop_str.contains("\u{958b}\u{767a}\u{74b0}\u{5883}"));

        let expected_jdk_slash = to_batch_safe_path(&jdk_str).replace('\\', "/");
        assert_eq!(
            extract_property_value(&prop_file, "filter.javahome.baseDirectory.value"),
            Some(expected_jdk_slash.clone())
        );
        assert_eq!(
            extract_property_value(&prop_file, "filter.shellscript.javaHome.value"),
            Some(expected_jdk_slash)
        );
        assert_eq!(
            extract_property_value(&prop_file, "custom.label"),
            Some("\u{8ca9}\u{4fc3}\u{30d0}\u{30c3}\u{30c1}".to_string())
        );

        let updated_xml_bytes = std::fs::read(build_dir.join("build.xml")).unwrap();
        let (updated_xml_str, xml_enc) = detect_and_decode_text(&updated_xml_bytes);
        assert_eq!(xml_enc, TextEncoding::ShiftJis);
        assert!(updated_xml_str.contains("\u{65e5}\u{672c}\u{8a9e}\u{30b3}\u{30e1}\u{30f3}\u{30c8}"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_discover_dynamic_log_filenames() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_dyn_logs_{}", unique));
        let proj_a = base.join("proj_a");
        let props_a = proj_a.join("batch").join("properties");
        std::fs::create_dir_all(&props_a).unwrap();
        std::fs::write(
            props_a.join("stcliblog4j.properties.filter"),
            "log4j.appender.stclibApp.File = @pom.log4j.path.value@/stclibApp.log\nlog4j.appender.stclibBatch.File = @pom.log4j.path.value@/batch.log\n",
        )
        .unwrap();

        let (b_a, app_a) = discover_configured_log_files(&proj_a.to_string_lossy(), None);
        assert_eq!(b_a, Some("batch.log".to_string()));
        assert_eq!(app_a, Some("stclibApp.log".to_string()));

        let proj_empty = base.join("proj_empty");
        std::fs::create_dir_all(&proj_empty).unwrap();
        let (b_e, app_e) = discover_configured_log_files(&proj_empty.to_string_lossy(), None);
        assert_eq!(b_e, None);
        assert_eq!(app_e, None);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_execute_bat_group_step_sequential_and_error() {
        use crate::modules::encoding::*;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("bh_group_step_{}_\u{65e5}\u{672c}\u{8a9e}", unique));
        std::fs::create_dir_all(&dir).unwrap();

        let ok_bat = dir.join("01_\u{6b63}\u{5e38}.bat");
        let fail_bat = dir.join("02_\u{30a8}\u{30e9}\u{30fc}.bat");

        let ok_content = "@echo off\r\necho \u{6b63}\u{5e38}\u{7d42}\u{4e86}\r\nexit /b 0\r\n";
        let fail_content = "@echo off\r\necho \u{5931}\u{6557}\u{30c6}\u{30b9}\u{30c8}\r\nexit /b 2\r\n";

        std::fs::write(&ok_bat, encode_text(ok_content, TextEncoding::ShiftJis).unwrap()).unwrap();
        std::fs::write(&fail_bat, encode_text(fail_content, TextEncoding::ShiftJis).unwrap()).unwrap();

        let default_setting = crate::modules::app_config::UserSetting::default();
        let step1 = execute_bat_group_step_with_setting(&ok_bat.to_string_lossy(), None, &default_setting);
        assert!(step1.success);
        assert_eq!(step1.exit_code, Some(0));
        assert!(step1.stdout.contains("\u{6b63}\u{5e38}\u{7d42}\u{4e86}"));
        assert!(!step1.stdout.contains("????"));

        let step2 = execute_bat_group_step_with_setting(&fail_bat.to_string_lossy(), None, &default_setting);
        assert!(!step2.success);
        assert_eq!(step2.exit_code, Some(2));
        assert!(step2.stdout.contains("\u{5931}\u{6557}\u{30c6}\u{30b9}\u{30c8}"));
        assert!(step2.error_message.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_default_txt_editor() {
        let editor = resolve_default_txt_editor();
        println!("OS default .txt editor: {:?}", editor);
        #[cfg(target_os = "windows")]
        assert!(editor.is_some());
    }

    #[test]
    fn test_scan_bat_files_rejects_other_project_and_expands_short_path() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let raw_base = std::env::temp_dir().join(format!("bh_cross_proj_{}", unique));
        std::fs::create_dir_all(&raw_base).unwrap();
        let base = to_display_long_path(&raw_base);

        let proj_master = base.join("mdw_corepkgmaster-batch");
        let master_bat_dir = proj_master.join("target").join("batch").join("batfiles");
        std::fs::create_dir_all(&master_bat_dir).unwrap();
        std::fs::write(master_bat_dir.join("master.bat"), "@echo off\r\n").unwrap();

        let proj_syohin = base.join("space folder").join("mdw_corepkgsyohinjiseki-batch");
        std::fs::create_dir_all(&proj_syohin).unwrap();

        let syohin_str = proj_syohin.to_string_lossy().to_string();
        let master_base_str = proj_master.join("target").to_string_lossy().to_string();

        // Even if properties base_dir points to another project, never return master's .bat files
        let (dir_mismatch, files_mismatch) = scan_bat_files(&syohin_str, Some(&master_base_str));
        assert!(dir_mismatch.is_none());
        assert!(files_mismatch.is_empty());

        // When syohin target exists, always return syohin's own clean long path even if base_dir has 8.3 short names
        let syohin_bat_dir = proj_syohin.join("target").join("batch").join("batfiles");
        std::fs::create_dir_all(&syohin_bat_dir).unwrap();
        std::fs::write(syohin_bat_dir.join("syohin.bat"), "@echo off\r\n").unwrap();
        let short_syohin_target = to_batch_safe_path(&proj_syohin.join("target").to_string_lossy());

        let (dir_ok, files_ok) = scan_bat_files(&syohin_str, Some(&short_syohin_target));
        let resolved_dir = dir_ok.expect("Expected syohin batfiles folder");
        assert_eq!(files_ok.len(), 1);
        assert_eq!(files_ok[0].name, "syohin.bat");
        assert!(
            resolved_dir.to_string_lossy().contains("space folder"),
            "Expected expanded long folder name, got: {}",
            resolved_dir.display()
        );
        assert!(
            !resolved_dir.to_string_lossy().to_lowercase().contains("mdw_corepkgmaster-batch"),
            "Unexpected master project path: {}",
            resolved_dir.display()
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_resolve_target_logs_rejects_other_project() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let raw_base = std::env::temp_dir().join(format!("bh_cross_logs_{}", unique));
        std::fs::create_dir_all(&raw_base).unwrap();
        let base = to_display_long_path(&raw_base);

        let proj_master = base.join("mdw_corepkgmaster-batch");
        let master_log_dir = proj_master.join("target").join("log");
        std::fs::create_dir_all(&master_log_dir).unwrap();
        std::fs::write(master_log_dir.join("batchApp.log"), "master log").unwrap();

        let proj_syohin = base.join("mdw_corepkgsyohinjiseki-batch");
        let filters_dir = proj_syohin.join("build").join("filters");
        std::fs::create_dir_all(&filters_dir).unwrap();
        let prop_file = filters_dir.join("development.properties");
        let prop_content = format!(
            "pom.log4j.path.value={}/\nfilter.shellscript.baseDirectory.value={}\n",
            master_log_dir.to_string_lossy().replace('\\', "/"),
            proj_master.join("target").to_string_lossy().replace('\\', "/")
        );
        std::fs::write(&prop_file, prop_content).unwrap();

        let syohin_str = proj_syohin.to_string_lossy().to_string();
        let (found_log, _) = resolve_log_file(
            &syohin_str,
            Some(&prop_file),
            Some(&proj_master.join("target").to_string_lossy()),
            "batchApp.log",
        );
        assert!(
            found_log.is_none(),
            "Should never resolve log from another project: {:?}",
            found_log
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_inspect_database_config_sqlserver_variants() {
        use crate::modules::db_manager::*;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_db_inspect_{}", unique));

        // Variant A: instance embedded in pom.db.host.value (10.51.162.54\\YSZY_UT)
        let proj_a = base.join("proj_a");
        std::fs::create_dir_all(proj_a.join("build").join("filters")).unwrap();
        std::fs::create_dir_all(proj_a.join("batch").join("properties")).unwrap();
        std::fs::write(
            proj_a.join("build").join("build.xml"),
            "<project name=\"a\" default=\"development-package\"><target name=\"development-package\"></target></project>",
        )
        .unwrap();
        std::fs::write(
            proj_a.join("build").join("filters").join("development.properties"),
            "pom.db.host.value=10.51.162.54\\\\YSZY_UT\npom.db.database.value=UT_013\npom.db.user.value=UT_013\npom.db.pswd.value=SECRET1\npom.db.init.value=1\npom.db.max.value=10\n",
        )
        .unwrap();
        std::fs::write(
            proj_a.join("batch").join("properties").join("database.properties.filter"),
            "database_name,rbsite_ora\ndriver,com.microsoft.sqlserver.jdbc.SQLServerDriver\nurl,jdbc:sqlserver://@pom.db.host.value@;integratedSecurity=false;databaseName=@pom.db.database.value@\nuser,@pom.db.user.value@\npswd,@pom.db.pswd.value@\ninitarray,@pom.db.init.value@\nmaxarray,@pom.db.max.value@\n",
        )
        .unwrap();

        let info_a = inspect_database_config(&proj_a.to_string_lossy(), "development-package");
        assert!(info_a.found);
        assert_eq!(info_a.db_type, "SQL Server");
        assert_eq!(info_a.effective_server.as_deref(), Some("10.51.162.54\\YSZY_UT"));
        assert_eq!(info_a.database.as_deref(), Some("UT_013"));
        assert_eq!(info_a.user.as_deref(), Some("UT_013"));
        assert_eq!(info_a.password.as_deref(), Some("SECRET1"));
        assert_eq!(info_a.init_pool.as_deref(), Some("1"));
        assert_eq!(info_a.max_pool.as_deref(), Some("10"));
        assert!(info_a.warning_message.is_none());

        // Variant B: separate host + port/serviceName & unresolved token warning
        let proj_b = base.join("proj_b");
        std::fs::create_dir_all(proj_b.join("build").join("filters")).unwrap();
        std::fs::create_dir_all(proj_b.join("batch").join("properties")).unwrap();
        std::fs::write(
            proj_b.join("build").join("build.xml"),
            "<project name=\"b\" default=\"development-package\"><target name=\"development-package\"></target></project>",
        )
        .unwrap();
        std::fs::write(
            proj_b.join("build").join("filters").join("development.properties"),
            "pom.db.host.value=172.16.0.233\npom.db.port.value=MDINF01\npom.db.serviceName.value=MDAUTO_QT\npom.db.user.value=USR1\npom.db.pswd.value=PWD1\n",
        )
        .unwrap();
        std::fs::write(
            proj_b.join("batch").join("properties").join("database.properties.filter"),
            "driver,com.microsoft.sqlserver.jdbc.SQLServerDriver\nurl,jdbc:sqlserver://@pom.db.host.value@\\@pom.db.port.value@;databaseName=@pom.db.name.value@\n",
        )
        .unwrap();

        let info_b = inspect_database_config(&proj_b.to_string_lossy(), "development-package");
        assert!(info_b.found);
        assert_eq!(info_b.effective_server.as_deref(), Some("172.16.0.233\\MDINF01"));
        assert_eq!(info_b.database.as_deref(), Some("MDAUTO_QT"));
        assert!(info_b.warning_message.is_some());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_database_connection_invalid_port_fast_fail() {
        use crate::modules::db_manager::*;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("bh_db_fail_{}", unique));
        std::fs::create_dir_all(root.join("build").join("filters")).unwrap();
        std::fs::create_dir_all(root.join("batch").join("properties")).unwrap();
        std::fs::write(
            root.join("build").join("build.xml"),
            "<project name=\"t\" default=\"development-package\"><target name=\"development-package\"></target></project>",
        )
        .unwrap();
        std::fs::write(
            root.join("build").join("filters").join("development.properties"),
            "pom.db.host.value=127.0.0.1\npom.db.port.value=1\npom.db.serviceName.value=ORCL\npom.db.user.value=scott\npom.db.pswd.value=tiger\n",
        )
        .unwrap();
        std::fs::write(
            root.join("batch").join("properties").join("database.properties.filter"),
            "driver,oracle.jdbc.driver.OracleDriver\nurl,jdbc:oracle:thin:@@pom.db.host.value@:@pom.db.port.value@:@pom.db.serviceName.value@\n",
        )
        .unwrap();

        let res = test_database_connection(&root.to_string_lossy(), "development-package");
        assert!(!res.success);
        assert_eq!(res.db_type, "Oracle");
        assert!(!res.message.trim().is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_batch_groups_persistence_and_project_isolation() {
        let original = load_settings();
        let proj_a = r"C:\__bh_group_test_proj_a__";
        let proj_b = r"C:\__bh_group_test_proj_b__";

        // Clean any leftover test keys
        for g in get_project_groups(proj_a) {
            let _ = delete_batch_group(proj_a, &g.id);
        }
        for g in get_project_groups(proj_b) {
            let _ = delete_batch_group(proj_b, &g.id);
        }

        // Create group in project A with duplicates in selection -> deduped & ordered
        let created_a = create_batch_group(
            proj_a,
            "Daily Batch",
            vec![
                r"C:\__bh_group_test_proj_a__\target\batch\batfiles\01_First.bat".to_string(),
                r"C:\__bh_group_test_proj_a__\target\batch\batfiles\02_Second.bat".to_string(),
                r"c:\__bh_group_test_proj_a__\target\batch\batfiles\01_First.bat".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(created_a.len(), 1);
        assert_eq!(created_a[0].name, "Daily Batch");
        assert_eq!(created_a[0].files.len(), 2);
        assert!(created_a[0].files[0].ends_with("01_First.bat"));
        assert!(created_a[0].files[1].ends_with("02_Second.bat"));

        // Duplicate group name rejected within same project
        let dup_err = create_batch_group(
            proj_a,
            "daily batch",
            vec![r"C:\__bh_group_test_proj_a__\03_Third.bat".to_string()],
        );
        assert!(dup_err.is_err());

        // Project B has independent groups (same name allowed independently)
        let created_b = create_batch_group(
            proj_b,
            "Daily Batch",
            vec![r"C:\__bh_group_test_proj_b__\target\batch\batfiles\B_Only.bat".to_string()],
        )
        .unwrap();
        assert_eq!(created_b.len(), 1);
        assert_eq!(get_project_groups(proj_a).len(), 1);
        assert_eq!(get_project_groups(proj_b).len(), 1);

        // Append to existing group in project A: preserves existing files, appends new, prevents duplicates
        let group_a_id = created_a[0].id.clone();
        let appended_a = add_files_to_batch_group(
            proj_a,
            &group_a_id,
            vec![
                r"C:\__bh_group_test_proj_a__\target\batch\batfiles\02_Second.bat".to_string(),
                r"C:\__bh_group_test_proj_a__\target\batch\batfiles\03_Third.bat".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(appended_a[0].files.len(), 3);
        assert!(appended_a[0].files[0].ends_with("01_First.bat"));
        assert!(appended_a[0].files[1].ends_with("02_Second.bat"));
        assert!(appended_a[0].files[2].ends_with("03_Third.bat"));

        // Reload from disk verifies persistence
        let reloaded_a = get_project_groups(proj_a);
        assert_eq!(reloaded_a.len(), 1);
        assert_eq!(reloaded_a[0].files.len(), 3);

        let _ = save_settings(&original);
    }

    #[test]
    fn test_auto_save_image_paths_overwrite_and_compat() {
        use crate::modules::app_config::*;

        // Backward-compatible deserialization of legacy setting.json without auto_save_image / screenshot_save_dir
        let legacy_json = r#"{"ps_window_width": 900, "ps_window_height": 600, "auto_capture_clipboard": false}"#;
        let parsed: UserSetting = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed.ps_window_width, 900);
        assert_eq!(parsed.ps_window_height, 600);
        assert!(!parsed.auto_capture_clipboard);
        assert!(!parsed.auto_save_image);
        assert_eq!(parsed.screenshot_save_dir, None);

        // Default screenshot root MUST be in setting.json's parent directory / image
        let expected_parent = get_setting_file_path().parent().unwrap().to_path_buf();
        let def_root = get_default_screenshot_root_dir();
        assert_eq!(def_root, expected_parent.join("image"));

        let default_setting = UserSetting {
            auto_save_image: true,
            screenshot_save_dir: None,
            ..UserSetting::default()
        };
        let out_default = resolve_screenshot_output_path(
            &default_setting,
            Some(r"C:\work\yoshizuya-corepkg"),
            r"C:\work\yoshizuya-corepkg\target\batch\batfiles\MSTB590021_CreateSGTorihikisaki.bat",
        );
        assert_eq!(
            out_default,
            def_root
                .join("yoshizuya-corepkg")
                .join("MSTB590021_CreateSGTorihikisaki.png")
        );

        // Deterministic overwrite: calling again produces identical path with no timestamps
        let out_default_2 = resolve_screenshot_output_path(
            &default_setting,
            Some(r"C:\work\yoshizuya-corepkg"),
            r"C:\work\yoshizuya-corepkg\target\batch\batfiles\MSTB590021_CreateSGTorihikisaki.bat",
        );
        assert_eq!(out_default, out_default_2);

        // Custom root folder check + writable directory validation
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let custom_root = std::env::temp_dir().join(format!("bh_custom_shot_{}", unique));
        validate_writable_directory(&custom_root).unwrap();
        let custom_setting = UserSetting {
            auto_save_image: true,
            screenshot_save_dir: Some(custom_root.to_string_lossy().to_string()),
            ..UserSetting::default()
        };
        let out_custom = resolve_screenshot_output_path(
            &custom_setting,
            Some(r"D:\repo\mdw_corepkgmaster-batch"),
            r"D:\repo\mdw_corepkgmaster-batch\target\batch\batfiles\Job01.bat",
        );
        assert_eq!(
            out_custom,
            custom_root.join("mdw_corepkgmaster-batch").join("Job01.png")
        );

        let _ = std::fs::remove_dir_all(&custom_root);
    }

    #[test]
    fn test_powershell_auto_save_image_script() {
        use crate::modules::app_config::UserSetting;

        // Clipboard OFF, Auto Save Image ON -> prompt hook active, saves PNG, no Clipboard call
        let save_only_setting = UserSetting {
            ps_window_width: 1024,
            ps_window_height: 680,
            auto_capture_clipboard: false,
            auto_save_image: true,
            screenshot_save_dir: Some(r"C:\shots".to_string()),
        };
        let s1 = build_powershell_run_script(
            r"C:\proj\target\batch\batfiles",
            "MSTB590021.bat",
            &save_only_setting,
            Some(r"C:\shots\proj\MSTB590021.png"),
            Some(r"C:\temp\status.txt"),
        );
        assert!(s1.contains("function global:prompt"));
        assert!(s1.contains("MSTB590021.png"));
        assert!(s1.contains("[System.Drawing.Imaging.ImageFormat]::Png"));
        assert!(!s1.contains("Clipboard"));

        // Sequential group step script contains output + metadata + PNG save + centered MoveWindow
        let g1 = build_powershell_group_step_script(
            r"C:\proj\target\batch\batfiles",
            "MSTB590022.bat",
            &save_only_setting,
            r"C:\shots\proj\MSTB590022.png",
            r"C:\temp\out.txt",
            r"C:\temp\meta.json",
        );
        assert!(g1.contains("MSTB590022.png"));
        assert!(g1.contains("screenshot_saved"));
        assert!(g1.contains("PrimaryScreen.WorkingArea"));
        assert!(g1.contains("MoveWindow"));
    }

    #[test]
    fn test_execute_bat_group_step_missing_file_not_skipped() {
        let missing_path = r"C:\__bh_nonexistent_dir__\missing_batch_001.bat";
        let default_setting = crate::modules::app_config::UserSetting::default();
        let res = execute_bat_group_step_with_setting(missing_path, None, &default_setting);
        assert!(!res.success);
        assert_eq!(res.name, "missing_batch_001.bat");
        assert!(res.error_message.unwrap_or_default().contains("does not exist"));
        assert!(!res.screenshot_saved);
    }

    #[test]
    fn test_auto_save_image_default_off_and_off_produces_no_png() {
        use crate::modules::app_config::*;

        let defaults = UserSetting::default();
        assert!(!defaults.auto_save_image);

        // Relative screenshot_save_dir must anchor to setting.json directory, NEVER process CWD
        let rel_setting = UserSetting {
            auto_save_image: true,
            screenshot_save_dir: Some("my_relative_shots".to_string()),
            ..UserSetting::default()
        };
        let setting_dir = get_setting_file_path().parent().unwrap().to_path_buf();
        assert_eq!(
            resolve_screenshot_root_dir(&rel_setting),
            setting_dir.join("my_relative_shots")
        );

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_off_{}", unique));
        let proj_dir = base.join("off_proj");
        let bat_dir = proj_dir.join("target").join("batch").join("batfiles");
        let shot_root = base.join("never_created_shots");
        std::fs::create_dir_all(&bat_dir).unwrap();
        let bat_file = bat_dir.join("OffJob.bat");
        std::fs::write(&bat_file, "@echo off\r\necho OFF_TEST\r\nexit /b 0\r\n").unwrap();

        let off_setting = UserSetting {
            auto_capture_clipboard: false,
            auto_save_image: false,
            screenshot_save_dir: Some(shot_root.to_string_lossy().to_string()),
            ..UserSetting::default()
        };

        let res = execute_bat_group_step_with_setting(
            &bat_file.to_string_lossy(),
            Some(&proj_dir.to_string_lossy()),
            &off_setting,
        );
        assert!(res.success);
        assert_eq!(res.exit_code, Some(0));
        assert!(!res.screenshot_saved);
        assert!(res.screenshot_path.is_none());
        assert!(!shot_root.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_auto_save_image_physical_png_creation_missing_dir_and_overwrite() {
        use crate::modules::app_config::UserSetting;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_png_real_{}", unique));
        let proj_dir = base
            .join("OneDrive - \u{682a}\u{5f0f}\u{4f1a}\u{793e}")
            .join("demo_proj");
        let bat_dir = proj_dir.join("target").join("batch").join("batfiles");
        let missing_shot_root = base.join("missing_parent").join("image_root");
        std::fs::create_dir_all(&bat_dir).unwrap();

        let bat_file = bat_dir.join("JobAlpha.bat");
        std::fs::write(&bat_file, "@echo off\r\necho FIRST_RUN_OK\r\nexit 0\r\n").unwrap();

        let on_setting = UserSetting {
            ps_window_width: 900,
            ps_window_height: 540,
            auto_capture_clipboard: false,
            auto_save_image: true,
            screenshot_save_dir: Some(missing_shot_root.to_string_lossy().to_string()),
        };

        // Run 1: destination folder does not exist yet -> auto-created + physical PNG created
        let res1 = execute_bat_group_step_with_setting(
            &bat_file.to_string_lossy(),
            Some(&proj_dir.to_string_lossy()),
            &on_setting,
        );
        assert!(res1.success, "Expected step1 success: {:?}", res1);
        assert_eq!(res1.exit_code, Some(0));
        assert!(
            res1.screenshot_saved,
            "Expected PNG to be saved physically, err: {:?}",
            res1.screenshot_error
        );
        assert!(res1.screenshot_error.is_none());

        let expected_png = missing_shot_root.join("demo_proj").join("JobAlpha.png");
        assert!(expected_png.is_file(), "PNG not found at {:?}", expected_png);
        let png_bytes_1 = std::fs::read(&expected_png).unwrap();
        assert!(png_bytes_1.len() > 500);
        assert_eq!(
            &png_bytes_1[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );

        // Run 2: same filename already exists -> must overwrite cleanly with zero duplicate files
        std::fs::write(
            &bat_file,
            "@echo off\r\necho SECOND_RUN_OVERWRITE_CONTENT_LINE\r\necho LINE_2\r\nexit 0\r\n",
        )
        .unwrap();
        let res2 = execute_bat_group_step_with_setting(
            &bat_file.to_string_lossy(),
            Some(&proj_dir.to_string_lossy()),
            &on_setting,
        );
        assert!(res2.success);
        assert!(res2.screenshot_saved, "Overwrite failed: {:?}", res2.screenshot_error);

        let png_bytes_2 = std::fs::read(&expected_png).unwrap();
        assert!(png_bytes_2.len() > 500);
        assert_eq!(
            &png_bytes_2[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
        let entries_in_proj_shots: Vec<_> = std::fs::read_dir(missing_shot_root.join("demo_proj"))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries_in_proj_shots.len(), 1);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_auto_save_image_individual_run_physical_png_and_jp_path() {
        use crate::modules::app_config::{resolve_screenshot_output_path, UserSetting};

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_single_run_{}", unique));
        let proj_dir = base
            .join("OneDrive - \u{682a}\u{5f0f}\u{4f1a}\u{793e}")
            .join("single_proj");
        let bat_dir = proj_dir.join("target").join("batch").join("batfiles");
        let shot_root = base.join("single_shots");
        std::fs::create_dir_all(&bat_dir).unwrap();

        let bat_file = bat_dir.join("SingleRunJob.bat");
        std::fs::write(&bat_file, "@echo off\r\necho SINGLE_RUN_OK\r\nexit 0\r\n").unwrap();

        let on_setting = UserSetting {
            ps_window_width: 920,
            ps_window_height: 540,
            auto_capture_clipboard: false,
            auto_save_image: true,
            screenshot_save_dir: Some(shot_root.to_string_lossy().to_string()),
        };

        let target_png = resolve_screenshot_output_path(
            &on_setting,
            Some(&proj_dir.to_string_lossy()),
            &bat_file.to_string_lossy(),
        );
        std::fs::create_dir_all(target_png.parent().unwrap()).unwrap();
        let status_file = base.join("single_meta.json");

        let raw_script = build_powershell_run_script(
            &bat_dir.to_string_lossy(),
            "SingleRunJob.bat",
            &on_setting,
            Some(&target_png.to_string_lossy()),
            Some(&status_file.to_string_lossy()),
        );
        let ps1_path = allocate_temp_ps1_path("bh_test_single");
        let full_script = append_ps1_self_cleanup(&raw_script, &ps1_path);
        write_ps1_with_bom(&ps1_path, &full_script).unwrap();

        let ps1_str = ps1_path.to_string_lossy().to_string();
        let mut child = spawn_conhost_powershell(
            &bat_dir,
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-NoExit",
                "-STA",
                "-File",
                &ps1_str,
            ],
        )
        .expect("Failed to start conhost PowerShell for single run test");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !status_file.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(150));
        }

        let _ = child.kill();
        let _ = child.wait();

        assert!(status_file.is_file(), "Single-run metadata JSON was never written");
        let meta_txt = std::fs::read_to_string(&status_file).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(meta_txt.trim_start_matches('\u{feff}').trim()).unwrap();
        assert_eq!(parsed["exit_code"].as_i64(), Some(0));
        assert_eq!(parsed["screenshot_saved"].as_bool(), Some(true));
        assert!(parsed["screenshot_error"].is_null());
        assert!(target_png.is_file(), "Single-run PNG missing: {:?}", target_png);
        let png_bytes = std::fs::read(&target_png).unwrap();
        assert!(png_bytes.len() > 500);
        assert_eq!(
            &png_bytes[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
        assert!(!ps1_path.exists(), "Temp .ps1 script should self-clean");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_auto_save_image_failure_does_not_corrupt_bat_result() {
        use crate::modules::app_config::UserSetting;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_shot_err_{}", unique));
        let proj_dir = base.join("err_proj");
        std::fs::create_dir_all(&proj_dir).unwrap();

        // Point screenshot_save_dir at a regular file so directory creation fails cleanly
        let blocking_file = base.join("not_a_directory.txt");
        std::fs::write(&blocking_file, b"blocker").unwrap();

        let bat_file = proj_dir.join("HealthyJob.bat");
        std::fs::write(&bat_file, "@echo off\r\necho BAT_STILL_OK\r\nexit /b 0\r\n").unwrap();

        let bad_shot_setting = UserSetting {
            auto_capture_clipboard: false,
            auto_save_image: true,
            screenshot_save_dir: Some(blocking_file.to_string_lossy().to_string()),
            ..UserSetting::default()
        };

        let res = execute_bat_group_step_with_setting(
            &bat_file.to_string_lossy(),
            Some(&proj_dir.to_string_lossy()),
            &bad_shot_setting,
        );
        assert!(res.success);
        assert_eq!(res.exit_code, Some(0));
        assert!(res.stdout.contains("BAT_STILL_OK"));
        assert!(!res.screenshot_saved);
        assert!(res.screenshot_error.is_some());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_sequential_run_all_continues_after_failure_with_auto_save_image() {
        use crate::modules::app_config::UserSetting;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_run_all_{}", unique));
        let proj_dir = base.join("batch_suite_proj");
        let bat_dir = proj_dir.join("target").join("batch").join("batfiles");
        let shot_root = base.join("suite_shots");
        std::fs::create_dir_all(&bat_dir).unwrap();

        let bat1 = bat_dir.join("01_Pass.bat");
        let bat2 = bat_dir.join("02_Fail.bat");
        let bat3 = bat_dir.join("03_PassAfterFail.bat");
        std::fs::write(&bat1, "@echo off\r\necho STEP1_OK\r\nexit 0\r\n").unwrap();
        std::fs::write(&bat2, "@echo off\r\necho STEP2_FAIL\r\nexit 2\r\n").unwrap();
        std::fs::write(&bat3, "@echo off\r\necho STEP3_OK\r\nexit 0\r\n").unwrap();

        let on_setting = UserSetting {
            ps_window_width: 880,
            ps_window_height: 520,
            auto_capture_clipboard: false,
            auto_save_image: true,
            screenshot_save_dir: Some(shot_root.to_string_lossy().to_string()),
        };

        let results: Vec<_> = [&bat1, &bat2, &bat3]
            .iter()
            .map(|b| {
                execute_bat_group_step_with_setting(
                    &b.to_string_lossy(),
                    Some(&proj_dir.to_string_lossy()),
                    &on_setting,
                )
            })
            .collect();

        assert!(results[0].success && results[0].screenshot_saved);
        assert!(!results[1].success && results[1].exit_code == Some(2) && results[1].screenshot_saved);
        assert!(results[2].success && results[2].screenshot_saved);

        for name in ["01_Pass.png", "02_Fail.png", "03_PassAfterFail.png"] {
            let p = shot_root.join("batch_suite_proj").join(name);
            assert!(p.is_file(), "Missing sequential screenshot: {:?}", p);
            let bytes = std::fs::read(&p).unwrap();
            assert_eq!(&bytes[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_normalize_project_path_equivalence() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let raw_base = std::env::temp_dir().join(format!("bh_norm_tab_{}", unique));
        let proj = raw_base.join("space proj").join("my-batch-project");
        std::fs::create_dir_all(&proj).unwrap();

        let canonical = normalize_project_path(&proj.to_string_lossy());
        let forward_slash = normalize_project_path(&format!(
            "\"{}\\\\\"",
            proj.to_string_lossy().replace('\\', "/")
        ));
        let upper_case = normalize_project_path(&proj.to_string_lossy().to_uppercase());
        let dot_dot = normalize_project_path(&format!(
            "{}\\..\\my-batch-project\\.",
            proj.to_string_lossy()
        ));
        let short_form = normalize_project_path(&to_batch_safe_path(&proj.to_string_lossy()));

        assert!(!canonical.key.is_empty());
        assert_eq!(canonical.key, forward_slash.key);
        assert_eq!(canonical.key, upper_case.key);
        assert_eq!(canonical.key, dot_dot.key);
        assert_eq!(canonical.key, short_form.key);

        let _ = std::fs::remove_dir_all(&raw_base);
    }

    #[test]
    fn test_clear_target_log_files_strict_and_safe() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_clear_logs_{}", unique));
        let proj = base.join("sample_proj");
        let target_log = proj.join("target").join("log");
        let source_log = proj.join("log");
        let bat_dir = proj.join("target").join("batch").join("batfiles");
        std::fs::create_dir_all(&target_log).unwrap();
        std::fs::create_dir_all(&source_log).unwrap();
        std::fs::create_dir_all(&bat_dir).unwrap();

        let log1 = target_log.join("batch.log");
        let log2 = target_log.join("stclibApp.log");
        let keep_src_log = source_log.join("keep.log");
        let keep_bat = bat_dir.join("KeepMe.bat");
        std::fs::write(&log1, "line 1\nline 2\nline 3\n").unwrap();
        std::fs::write(&log2, "app log line A\napp log line B\n").unwrap();
        std::fs::write(&keep_src_log, "DO_NOT_CLEAR_SOURCE_LOG").unwrap();
        std::fs::write(&keep_bat, "@echo off\r\necho KEEP_BAT\r\n").unwrap();

        let res = clear_target_log_files(
            Some(&proj.to_string_lossy()),
            None,
            None,
            Some("batch.log"),
            Some("stclibApp.log"),
        )
        .unwrap();

        assert!(res.failed_files.is_empty());
        assert_eq!(res.cleared_files.len(), 2);
        assert_eq!(std::fs::metadata(&log1).unwrap().len(), 0);
        assert_eq!(std::fs::metadata(&log2).unwrap().len(), 0);
        assert_eq!(
            std::fs::read_to_string(&keep_src_log).unwrap(),
            "DO_NOT_CLEAR_SOURCE_LOG"
        );
        assert_eq!(
            std::fs::read_to_string(&keep_bat).unwrap(),
            "@echo off\r\necho KEEP_BAT\r\n"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_powershell_dual_monitor_centering_and_physical_png_with_clipboard() {
        use crate::modules::app_config::UserSetting;

        let setting_both = UserSetting {
            ps_window_width: 1130,
            ps_window_height: 676,
            auto_capture_clipboard: true,
            auto_save_image: true,
            screenshot_save_dir: Some(r"C:\shots".to_string()),
        };
        let mon_script = build_powershell_run_script_on_monitor(
            r"C:\proj\target\batch\batfiles",
            "CenterTest.bat",
            &setting_both,
            Some(r"C:\shots\proj\CenterTest.png"),
            Some(r"C:\temp\meta.json"),
            Some((2880, 540)),
        );
        assert!(mon_script.contains("MonitorFromPoint"));
        assert!(mon_script.contains("GetMonitorInfoW"));
        assert!(mon_script.contains("2880"));
        assert!(mon_script.contains("540"));

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("bh_mon_png_{}", unique));
        let proj = base.join("dual_mon_proj");
        let bat_dir = proj.join("target").join("batch").join("batfiles");
        let shot_root = base.join("shots");
        std::fs::create_dir_all(&bat_dir).unwrap();
        let bat_file = bat_dir.join("DualMonJob.bat");
        std::fs::write(&bat_file, "@echo off\r\necho DUAL_MON_PNG_OK\r\nexit 0\r\n").unwrap();

        let live_setting = UserSetting {
            ps_window_width: 1130,
            ps_window_height: 676,
            auto_capture_clipboard: true,
            auto_save_image: true,
            screenshot_save_dir: Some(shot_root.to_string_lossy().to_string()),
        };

        let res = execute_bat_group_step_with_setting_on_monitor(
            &bat_file.to_string_lossy(),
            Some(&proj.to_string_lossy()),
            &live_setting,
            Some((960, 540)),
        );
        assert!(res.success, "Expected DualMonJob success: {:?}", res);
        assert!(
            res.screenshot_saved,
            "Expected PNG with clipboard+save both ON: {:?}",
            res.screenshot_error
        );
        let expected_png = shot_root.join("dual_mon_proj").join("DualMonJob.png");
        assert!(expected_png.is_file());
        let png_bytes = std::fs::read(&expected_png).unwrap();
        assert!(png_bytes.len() > 500);
        assert_eq!(&png_bytes[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

        let _ = std::fs::remove_dir_all(&base);
    }
}
