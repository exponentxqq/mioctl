/// Convert flag emoji (Regional Indicator pairs) to bracketed country codes.
///
/// Flag emoji are composed of two Regional Indicator characters (U+1F1E6 – U+1F1FF),
/// each encoding one uppercase ASCII letter (A=U+1F1E6, B=U+1F1E7, …, Z=U+1F1FF).
/// Many terminal fonts cannot render them, so we replace 🇯🇵 → [JP], 🇭🇰 → [HK], etc.
pub fn readable_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();

    while let Some(c) = chars.next() {
        if is_regional_indicator(c) {
            let letter1 = regional_to_ascii(c);
            if let Some(&next) = chars.peek() {
                if is_regional_indicator(next) {
                    let letter2 = regional_to_ascii(chars.next().unwrap());
                    result.push('[');
                    result.push(letter1);
                    result.push(letter2);
                    result.push(']');
                    continue;
                }
            }
            // Lone regional indicator — just push the letter
            result.push(letter1);
        } else {
            result.push(c);
        }
    }
    result
}

fn is_regional_indicator(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

fn regional_to_ascii(c: char) -> char {
    (b'A' + (c as u32 - 0x1F1E6) as u8) as char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_emoji_conversion() {
        // 🇯🇵 = U+1F1EF U+1F1F5
        let input = "\u{1F1EF}\u{1F1F5} Japan-01";
        assert_eq!(readable_name(input), "[JP] Japan-01");
    }

    #[test]
    fn test_hk_flag() {
        // 🇭🇰 = U+1F1ED U+1F1F0
        let input = "\u{1F1ED}\u{1F1F0} HongKong";
        assert_eq!(readable_name(input), "[HK] HongKong");
    }

    #[test]
    fn test_no_flags() {
        assert_eq!(readable_name("DIRECT"), "DIRECT");
    }

    #[test]
    fn test_multiple_flags() {
        // 🇺🇸🇬🇧
        let input = "\u{1F1FA}\u{1F1F8} US \u{1F1EC}\u{1F1E7} GB";
        assert_eq!(readable_name(input), "[US] US [GB] GB");
    }

    #[test]
    fn test_lone_regional_indicator() {
        let input = "\u{1F1EF} no pair";
        assert_eq!(readable_name(input), "J no pair");
    }

    #[test]
    fn test_empty() {
        assert_eq!(readable_name(""), "");
    }
}
