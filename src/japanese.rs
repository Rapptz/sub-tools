#[inline]
fn is_halfwidth_kana(c: char) -> bool {
    ('\u{FF65}'..='\u{FF9F}').contains(&c)
}

#[inline]
fn is_katakana(c: char) -> bool {
    ('\u{30A0}'..='\u{30FA}').contains(&c)
}

#[inline]
pub fn is_japanese(ch: char) -> bool {
    const CJK_MAPPING: [std::ops::RangeInclusive<char>; 3] = [
        '\u{3040}'..='\u{30ff}', // Hiragana + Katakana
        '\u{ff66}'..='\u{ff9d}', // Half-width Katakana
        '\u{4e00}'..='\u{9faf}', // Common + Uncommon Kanji
    ];
    CJK_MAPPING.iter().any(|c| c.contains(&ch))
}

fn replace_halfwith_kana(input: &str) -> Option<String> {
    let index = input.find(is_halfwidth_kana)?;
    let mut output = String::from(&input[..index]);
    output.reserve(input.len() - index);
    let chars = input[index..].chars();
    for ch in chars {
        match ch {
            '･' => output.push('・'),
            'ｦ' => output.push('ヲ'),
            'ｧ' => output.push('ァ'),
            'ｨ' => output.push('ィ'),
            'ｩ' => output.push('ゥ'),
            'ｪ' => output.push('ェ'),
            'ｫ' => output.push('ォ'),
            'ｬ' => output.push('ャ'),
            'ｭ' => output.push('ュ'),
            'ｮ' => output.push('ョ'),
            'ｯ' => output.push('ッ'),
            'ｰ' => output.push('ー'),
            'ｱ' => output.push('ア'),
            'ｲ' => output.push('イ'),
            'ｳ' => output.push('ウ'),
            'ｴ' => output.push('エ'),
            'ｵ' => output.push('オ'),
            'ｶ' => output.push('カ'),
            'ｷ' => output.push('キ'),
            'ｸ' => output.push('ク'),
            'ｹ' => output.push('ケ'),
            'ｺ' => output.push('コ'),
            'ｻ' => output.push('サ'),
            'ｼ' => output.push('シ'),
            'ｽ' => output.push('ス'),
            'ｾ' => output.push('セ'),
            'ｿ' => output.push('ソ'),
            'ﾀ' => output.push('タ'),
            'ﾁ' => output.push('チ'),
            'ﾂ' => output.push('ツ'),
            'ﾃ' => output.push('テ'),
            'ﾄ' => output.push('ト'),
            'ﾅ' => output.push('ナ'),
            'ﾆ' => output.push('ニ'),
            'ﾇ' => output.push('ヌ'),
            'ﾈ' => output.push('ネ'),
            'ﾉ' => output.push('ノ'),
            'ﾊ' => output.push('ハ'),
            'ﾋ' => output.push('ヒ'),
            'ﾌ' => output.push('フ'),
            'ﾍ' => output.push('ヘ'),
            'ﾎ' => output.push('ホ'),
            'ﾏ' => output.push('マ'),
            'ﾐ' => output.push('ミ'),
            'ﾑ' => output.push('ム'),
            'ﾒ' => output.push('メ'),
            'ﾓ' => output.push('モ'),
            'ﾔ' => output.push('ヤ'),
            'ﾕ' => output.push('ユ'),
            'ﾖ' => output.push('ヨ'),
            'ﾗ' => output.push('ラ'),
            'ﾘ' => output.push('リ'),
            'ﾙ' => output.push('ル'),
            'ﾚ' => output.push('レ'),
            'ﾛ' => output.push('ロ'),
            'ﾜ' => output.push('ワ'),
            'ﾝ' => output.push('ン'),
            // These two are a bit of a special case
            // They might cause some mess ups, but ideally it doesn't
            'ﾞ' => {
                // In the katakana table, a voiced dakuten
                // is merely +1 char from the previous one
                match output.pop() {
                    Some(ch) => {
                        if is_katakana(ch) {
                            output.push(char::from_u32(ch as u32 + 1).unwrap());
                        } else {
                            output.push(ch);
                            output.push('゛');
                        }
                    }
                    None => output.push('゛'),
                }
            }
            'ﾟ' => {
                // In the katakana table, a voiced dakuten
                // is merely +2 char from the previous one
                match output.pop() {
                    Some(ch) => {
                        if is_katakana(ch) {
                            output.push(char::from_u32(ch as u32 + 2).unwrap());
                        } else {
                            output.push(ch);
                            output.push('゜');
                        }
                    }
                    None => output.push('゜'),
                }
            }
            _ => output.push(ch),
        }
    }
    Some(output)
}

pub fn fix_broken_text(text: &mut String) {
    // Remove e.g. [外:37F6ECF37A0A3EF8DFF083CCC8754F81]-like instances of text
    if let Some(index) = text.find("[外:") {
        let sub = &text[index..];
        // Find the first instance that's not uppercase hex
        if let Some(end) = sub.find(|c: char| !c.is_ascii_hexdigit()) {
            if sub.as_bytes()[end] == b']' {
                text.drain(index..=end);
            }
        }
    }

    // Fix up half-width kana
    if let Some(result) = replace_halfwith_kana(text) {
        *text = result;
    }

    // Fix up &lrm; U+202A and U+202C characters
    *text = text
        .replace(['\u{202a}', '\u{202c}'], "")
        .replace("&lrm;", "");
}

pub fn contains_japanese(s: &str) -> bool {
    s.chars().any(is_japanese)
}
