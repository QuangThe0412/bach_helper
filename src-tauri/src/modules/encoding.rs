/// Text encoding utilities for Japanese Shift-JIS (CP932 / MS932), UTF-8, and Java .properties files.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    ShiftJis,
}

const CODEPAGE_SHIFT_JIS: u32 = 932;

/// Decode Shift-JIS / CP932 (MS932) bytes to Rust String without depending on process ACP.
pub fn decode_shift_jis(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return Some(String::new());
    }

    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn MultiByteToWideChar(
                CodePage: u32,
                dwFlags: u32,
                lpMultiByteStr: *const u8,
                cbMultiByte: i32,
                lpWideCharStr: *mut u16,
                cchWideChar: i32,
            ) -> i32;
        }

        const MB_ERR_INVALID_CHARS: u32 = 0x00000008;

        for &flags in &[MB_ERR_INVALID_CHARS, 0] {
            unsafe {
                let len = MultiByteToWideChar(
                    CODEPAGE_SHIFT_JIS,
                    flags,
                    bytes.as_ptr(),
                    bytes.len() as i32,
                    std::ptr::null_mut(),
                    0,
                );
                if len > 0 {
                    let mut wide = vec![0u16; len as usize];
                    let written = MultiByteToWideChar(
                        CODEPAGE_SHIFT_JIS,
                        flags,
                        bytes.as_ptr(),
                        bytes.len() as i32,
                        wide.as_mut_ptr(),
                        len,
                    );
                    if written > 0 {
                        return Some(String::from_utf16_lossy(&wide[..written as usize]));
                    }
                }
            }
        }
    }

    None
}

/// Encode text strictly as Windows CP932 / Shift-JIS.
/// Returns Err if any character cannot be represented in CP932 (prevents silent '?' replacement).
pub fn encode_shift_jis(text: &str) -> Result<Vec<u8>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }

    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn WideCharToMultiByte(
                CodePage: u32,
                dwFlags: u32,
                lpWideCharStr: *const u16,
                cchWideChar: i32,
                lpMultiByteStr: *mut u8,
                cbMultiByte: i32,
                lpDefaultChar: *const u8,
                lpUsedDefaultChar: *mut i32,
            ) -> i32;
        }

        let wide: Vec<u16> = text.encode_utf16().collect();
        if wide.is_empty() {
            return Ok(Vec::new());
        }

        unsafe {
            let mut used_default: i32 = 0;
            let len = WideCharToMultiByte(
                CODEPAGE_SHIFT_JIS,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                &mut used_default,
            );
            if len <= 0 {
                return Err("Failed to calculate Shift-JIS (CP932) byte length.".to_string());
            }

            let mut buf = vec![0u8; len as usize];
            used_default = 0;
            let written = WideCharToMultiByte(
                CODEPAGE_SHIFT_JIS,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                buf.as_mut_ptr(),
                len,
                std::ptr::null(),
                &mut used_default,
            );
            if written <= 0 {
                return Err("Failed to encode text as Shift-JIS (CP932).".to_string());
            }
            if used_default != 0 {
                return Err(
                    "Text contains Unicode characters not representable in Shift-JIS (CP932)."
                        .to_string(),
                );
            }
            buf.truncate(written as usize);
            Ok(buf)
        }
    }

    #[cfg(not(windows))]
    {
        if text.is_ascii() {
            Ok(text.as_bytes().to_vec())
        } else {
            Err("Non-ASCII Shift-JIS encoding requires Windows CP932 support.".to_string())
        }
    }
}

/// Detect encoding (UTF-8 BOM, UTF-8, or Shift-JIS/CP932) and decode full file bytes verbatim.
pub fn detect_and_decode_text(bytes: &[u8]) -> (String, TextEncoding) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(rest) = std::str::from_utf8(&bytes[3..]) {
            return (rest.to_string(), TextEncoding::Utf8Bom);
        }
    }

    if let Ok(utf8) = std::str::from_utf8(bytes) {
        return (utf8.to_string(), TextEncoding::Utf8);
    }

    if let Some(sjis) = decode_shift_jis(bytes) {
        return (sjis, TextEncoding::ShiftJis);
    }

    (
        String::from_utf8_lossy(bytes).to_string(),
        TextEncoding::Utf8,
    )
}

/// Encode text in the requested TextEncoding.
pub fn encode_text(text: &str, encoding: TextEncoding) -> Result<Vec<u8>, String> {
    match encoding {
        TextEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
        TextEncoding::Utf8Bom => {
            let mut out = Vec::with_capacity(3 + text.len());
            out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            out.extend_from_slice(text.as_bytes());
            Ok(out)
        }
        TextEncoding::ShiftJis => encode_shift_jis(text),
    }
}

/// Inspect an XML prolog (`<?xml ... encoding="..." ?>`) and return the declared encoding if present.
pub fn detect_xml_declared_encoding(text: &str, fallback: TextEncoding) -> TextEncoding {
    let head: String = text.chars().take(512).collect();
    let lower = head.to_lowercase();
    if let Some(decl_start) = lower.find("<?xml") {
        if let Some(decl_rel_end) = lower[decl_start..].find("?>") {
            let decl = &lower[decl_start..decl_start + decl_rel_end];
            if decl.contains("shift_jis")
                || decl.contains("shift-jis")
                || decl.contains("ms932")
                || decl.contains("windows-31j")
                || decl.contains("cp932")
            {
                return TextEncoding::ShiftJis;
            }
            if decl.contains("utf-8") || decl.contains("utf8") {
                return if fallback == TextEncoding::Utf8Bom {
                    TextEncoding::Utf8Bom
                } else {
                    TextEncoding::Utf8
                };
            }
        }
    }
    fallback
}

/// Escape non-ASCII characters in a Java `.properties` value as `\uXXXX`.
/// Java's `Properties.load(InputStream)` (used by Ant `<filterset><filtersfile>`) reads ISO-8859-1,
/// so `\uXXXX` guarantees Japanese characters never corrupt to `????`.
pub fn escape_properties_value(val: &str) -> String {
    let mut out = String::with_capacity(val.len());
    for ch in val.chars() {
        if (ch as u32) <= 0x7F {
            out.push(ch);
        } else {
            let mut buf = [0u16; 2];
            let units = ch.encode_utf16(&mut buf);
            for u in units {
                out.push_str(&format!("\\u{:04x}", u));
            }
        }
    }
    out
}

/// Decode Java `.properties` `\uXXXX` escape sequences back into UTF-8 Rust `String`.
/// Preserves literal backslashes in Windows paths when not followed by `\u` + 4 hex digits.
pub fn unescape_properties_value(val: &str) -> String {
    let chars: Vec<char> = val.chars().collect();
    let mut out = String::with_capacity(val.len());
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 5 < chars.len() && chars[i + 1] == 'u' {
            let hex: String = chars[i + 2..i + 6].iter().collect();
            if let Ok(code1) = u16::from_str_radix(&hex, 16) {
                if (0xD800..=0xDBFF).contains(&code1)
                    && i + 11 < chars.len()
                    && chars[i + 6] == '\\'
                    && chars[i + 7] == 'u'
                {
                    let hex2: String = chars[i + 8..i + 12].iter().collect();
                    if let Ok(code2) = u16::from_str_radix(&hex2, 16) {
                        if (0xDC00..=0xDFFF).contains(&code2) {
                            let combined =
                                0x10000 + (((code1 as u32 - 0xD800) << 10) | (code2 as u32 - 0xDC00));
                            if let Some(ch) = char::from_u32(combined) {
                                out.push(ch);
                                i += 12;
                                continue;
                            }
                        }
                    }
                }
                if let Some(ch) = char::from_u32(code1 as u32) {
                    out.push(ch);
                    i += 6;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}
