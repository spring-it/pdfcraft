//! Chinese presentation names for faces that ship without Chinese name records.
//! These labels never participate in font selection or PDF writing.

pub(super) fn han(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{323af}'))
}

fn zh(traditional: bool, simple: &'static str, traditional_name: &'static str) -> &'static str {
    if traditional { traditional_name } else { simple }
}

pub(super) fn fallback(name: &str, family: Option<&str>, traditional: bool) -> String {
    let name =
        name.split_once('+').filter(|(subset, _)| subset.len() == 6 && subset.bytes().all(|b| b.is_ascii_uppercase())).map_or(name, |(_, rest)| rest);
    if han(name) {
        return name.into();
    }

    // Ordered longest first wherever families share a prefix. Accept both the human
    // spelling and the PostScript spellings, including named weights on the face.
    const ALIASES: &[(&str, &str, &str)] = &[
        ("microsoftjhengheiui", "微软正黑体界面", "微軟正黑體介面"),
        ("microsoftjhenghei", "微软正黑体", "微軟正黑體"),
        ("microsoftyaheiui", "微软雅黑界面", "微軟雅黑介面"),
        ("microsoftyahei", "微软雅黑", "微軟雅黑"),
        ("timesnewroman", "新罗马体", "新羅馬體"),
        ("timesroman", "时代罗马体", "時代羅馬體"),
        ("sourcehansans", "思源黑体", "思源黑體"),
        ("sourcehanserif", "思源宋体", "思源宋體"),
        ("notosansarabic", "诺托阿拉伯文无衬线体", "諾托阿拉伯文無襯線體"),
        ("notosansarmenian", "诺托亚美尼亚文无衬线体", "諾托亞美尼亞文無襯線體"),
        ("notosansgeorgian", "诺托格鲁吉亚文无衬线体", "諾托喬治亞文無襯線體"),
        ("notosanshebrew", "诺托希伯来文无衬线体", "諾托希伯來文無襯線體"),
        ("notosansthai", "诺托泰文无衬线体", "諾托泰文無襯線體"),
        ("notosansdevanagari", "诺托天城文无衬线体", "諾托天城文無襯線體"),
        ("notocoloremoji", "诺托彩色表情", "諾托彩色表情"),
        ("notosanscjksc", "诺托简体黑体", "諾托簡體黑體"),
        ("notosanscjktc", "诺托繁体黑体", "諾托繁體黑體"),
        ("notosanscjkhk", "诺托香港黑体", "諾托香港黑體"),
        ("notosanscjkjp", "诺托日文黑体", "諾托日文黑體"),
        ("notosanscjkkr", "诺托韩文黑体", "諾托韓文黑體"),
        ("notosanscjk", "诺托黑体", "諾托黑體"),
        ("notoserifcjksc", "诺托简体宋体", "諾托簡體宋體"),
        ("notoserifcjktc", "诺托繁体宋体", "諾托繁體宋體"),
        ("notoserifcjkhk", "诺托香港宋体", "諾托香港宋體"),
        ("notoserifcjk", "诺托宋体", "諾托宋體"),
        ("notosanssc", "诺托简体无衬线体", "諾托簡體無襯線體"),
        ("notosanstc", "诺托繁体无衬线体", "諾托繁體無襯線體"),
        ("notosans", "诺托无衬线体", "諾托無襯線體"),
        ("notoserifsc", "诺托简体衬线体", "諾托簡體襯線體"),
        ("notoseriftc", "诺托繁体衬线体", "諾托繁體襯線體"),
        ("notoserif", "诺托衬线体", "諾托襯線體"),
        ("dejavusansmono", "德雅无衬线等宽体", "德雅無襯線等寬體"),
        ("dejavusans", "德雅无衬线体", "德雅無襯線體"),
        ("dejavuserif", "德雅衬线体", "德雅襯線體"),
        ("liberationmono", "解放等宽体", "解放等寬體"),
        ("liberationsans", "解放无衬线体", "解放無襯線體"),
        ("liberationserif", "解放衬线体", "解放襯線體"),
        ("jetbrainsmono", "捷脑等宽体", "捷腦等寬體"),
        ("amiriquran", "阿米里古兰经体", "阿米里古蘭經體"),
        ("amiri", "阿米里体", "阿米里體"),
        ("caladea", "卡拉迪亚体", "卡拉迪亞體"),
        ("carlito", "卡利托体", "卡利托體"),
        ("cascadiacode", "卡斯卡迪亚代码体", "卡斯卡迪亞程式碼體"),
        ("cascadiamono", "卡斯卡迪亚等宽体", "卡斯卡迪亞等寬體"),
        ("arialnarrow", "阿里尔窄体", "阿里爾窄體"),
        ("arial", "阿里尔体", "阿里爾體"),
        ("georgia", "乔治亚体", "喬治亞體"),
        ("helvetica", "赫尔维蒂卡体", "赫爾維蒂卡體"),
        ("helv", "赫尔维蒂卡体", "赫爾維蒂卡體"),
        ("couriernew", "新信使体", "新信使體"),
        ("courier", "信使等宽体", "信使等寬體"),
        ("cour", "信使等宽体", "信使等寬體"),
        ("tiro", "时代罗马体", "時代羅馬體"),
        ("times", "时代罗马体", "時代羅馬體"),
        ("calibri", "卡利布里体", "卡利布里體"),
        ("cambria", "坎布里亚体", "坎布里亞體"),
        ("consolas", "康索拉斯等宽体", "康索拉斯等寬體"),
        ("segoeui", "西戈界面字体", "西戈介面字型"),
        ("verdana", "维尔达纳体", "維爾達納體"),
        ("tahoma", "塔霍马体", "塔霍馬體"),
        ("inter", "因特无衬线体", "因特無襯線體"),
        ("lxgwwenkaitc", "霞鹜文楷繁体", "霞鶩文楷繁體"),
        ("lxgwwenkai", "霞鹜文楷", "霞鶩文楷"),
        ("nsimsun", "新宋体", "新宋體"),
        ("simsun", "宋体", "宋體"),
        ("simhei", "黑体", "黑體"),
        ("fangsonggb2312", "仿宋", "仿宋"),
        ("fangsong", "仿宋", "仿宋"),
        ("kaitigb2312", "楷体", "楷體"),
        ("kaiti", "楷体", "楷體"),
        ("pmingliu", "新细明体", "新細明體"),
        ("mingliu", "细明体", "細明體"),
        ("dengxian", "等线", "等線"),
        ("pingfang", "苹方", "蘋方"),
    ];
    let key = |s: &str| s.bytes().filter(u8::is_ascii_alphanumeric).map(|b| b.to_ascii_lowercase() as char).collect::<String>();
    let face_key = key(name);
    let family_key = family.map(key);
    let mut alias = None;
    for &(prefix, simple, trad) in ALIASES {
        if let Some(suffix) = face_key.strip_prefix(prefix)
            && (suffix.is_empty() || style(suffix, traditional).is_some() || family_key.as_deref() == Some(prefix))
        {
            alias = Some((zh(traditional, simple, trad), suffix));
            break;
        }
    }
    if let Some((family, suffix)) = alias {
        let variant = style(suffix, traditional);
        return match variant {
            Some("") => family.into(),
            Some(variant) => format!("{family} {variant}"),
            None => format!("{family}（{name}）"),
        };
    }

    // An arbitrary vendor face may not have an official Chinese name. Keep its actual
    // identity instead of inventing a translation or showing identical options.
    let base = family.filter(|f| han(f)).unwrap_or(zh(traditional, "字体", "字型"));
    let suffix = name.rsplit_once(['-', '_', ' ']).map(|(_, s)| key(s));
    match suffix.as_deref().and_then(|s| style(s, traditional)) {
        Some(s) if !s.is_empty() => format!("{base} {s}（{name}）"),
        _ => format!("{base}（{name}）"),
    }
}

fn style(suffix: &str, traditional: bool) -> Option<&'static str> {
    let (simple, trad) = match suffix {
        "" => ("", ""),
        "regular" | "normal" | "roman" | "book" | "mt" | "psmt" => ("常规", "標準"),
        "thin" => ("纤细", "纖細"),
        "extralight" | "ultralight" => ("特细", "特細"),
        "light" => ("细体", "細體"),
        "medium" => ("中等", "中等"),
        "semibold" | "demibold" => ("半粗体", "半粗體"),
        "bold" | "boldmt" => ("粗体", "粗體"),
        "extrabold" | "ultrabold" => ("特粗体", "特粗體"),
        "black" | "heavy" => ("超粗体", "超粗體"),
        "italic" | "italicmt" => ("斜体", "斜體"),
        "oblique" => ("倾斜体", "傾斜體"),
        "bolditalic" | "bolditalicmt" => ("粗斜体", "粗斜體"),
        "boldoblique" => ("粗倾斜体", "粗傾斜體"),
        "lightitalic" => ("细斜体", "細斜體"),
        "semibolditalic" => ("半粗斜体", "半粗斜體"),
        "condensed" => ("紧凑体", "緊湊體"),
        "condensedbold" => ("紧凑粗体", "緊湊粗體"),
        "sc" => ("简体", "簡體"),
        "tc" => ("繁体", "繁體"),
        "hk" => ("香港字形", "香港字形"),
        "jp" => ("日文字形", "日文字形"),
        "kr" => ("韩文字形", "韓文字形"),
        _ => return None,
    };
    Some(zh(traditional, simple, trad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_names_cover_pdf_presets_and_the_reported_faces() {
        for (name, expected) in [
            ("Helvetica", "赫尔维蒂卡体"),
            ("Times Roman", "时代罗马体"),
            ("Courier", "信使等宽体"),
            ("DejaVuSans-BoldOblique", "德雅无衬线体 粗倾斜体"),
            ("Amiri-BoldItalic", "阿米里体 粗斜体"),
            ("AmiriQuran-Regular", "阿米里古兰经体 常规"),
            ("Caladea-Bold", "卡拉迪亚体 粗体"),
            ("NotoSans-Bold", "诺托无衬线体 粗体"),
            ("NotoSansArmenian-Regular", "诺托亚美尼亚文无衬线体 常规"),
            ("NotoSansGeorgian-Bold", "诺托格鲁吉亚文无衬线体 粗体"),
            ("ABCDEF+SimSun", "宋体"),
            ("MicrosoftYaHei-Bold", "微软雅黑 粗体"),
        ] {
            assert_eq!(fallback(name, None, false), expected);
        }
        assert_eq!(fallback("Helvetica", None, true), "赫爾維蒂卡體");
    }

    #[test]
    fn preserves_unmapped_vendor_identity_and_existing_chinese_labels() {
        assert_eq!(fallback("VendorOne-Regular", None, false), "字体 常规（VendorOne-Regular）");
        assert_ne!(fallback("VendorOne-Regular", None, false), fallback("VendorTwo-Regular", None, false));
        assert_eq!(fallback("保持源字体", None, false), "保持源字体");
        assert_eq!(fallback("Vendor-Regular", Some("本机字体名"), true), "本机字体名 標準（Vendor-Regular）");
    }
}
