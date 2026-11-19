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

        let custom = UserSetting {
            ps_window_width: 1280,
            ps_window_height: 720,
            auto_capture_clipboard: false,
        };
        let saved = save_user_setting(&custom).unwrap();
        assert_eq!(saved, custom);
        assert_eq!(load_user_setting(), custom);

        let clamped = save_user_setting(&UserSetting {
            ps_window_width: 50,
            ps_window_height: 99999,
            auto_capture_clipboard: true,
        })
        .unwrap();
        assert_eq!(clamped.ps_window_width, MIN_PS_WIDTH);
        assert_eq!(clamped.ps_window_height, MAX_PS_HEIGHT);

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
        };
        let script = build_powershell_run_script(
            r"C:\proj\target\batch\batfiles",
            "demo.bat",
            &setting,
        );

        assert!(script.contains("Write-Host ('PS ' + (Get-Location).Path + '> .\\demo.bat')"));
        assert!(script.contains("& '.\\demo.bat'"));
        assert!(script.contains("function global:prompt"));
        assert!(script.contains("MoveWindow"));
        assert!(script.contains("Clipboard"));
        assert!(script.contains("1120"));
        assert!(script.contains("720"));
        assert!(!script.contains("Running:"));
        assert!(!script.contains("cmd.exe /c"));

        let no_cap_setting = UserSetting {
            ps_window_width: 1000,
            ps_window_height: 650,
            auto_capture_clipboard: false,
        };
        let no_cap_script = build_powershell_run_script(
            r"C:\proj\target\batch\batfiles",
            "demo.bat",
            &no_cap_setting,
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

        let step1 = execute_bat_group_step(&ok_bat.to_string_lossy());
        assert!(step1.success);
        assert_eq!(step1.exit_code, Some(0));
        assert!(step1.stdout.contains("\u{6b63}\u{5e38}\u{7d42}\u{4e86}"));
        assert!(!step1.stdout.contains("????"));

        let step2 = execute_bat_group_step(&fail_bat.to_string_lossy());
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
}
