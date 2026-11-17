//! Tidies recognised speech: drops hesitation sounds, repairs punctuation and removes stutters.
//!
//! The rules are deliberately narrow. Anything that could be a real word in some sentence is left alone;
//! this is not a rewrite, and the user can turn it off.

fn chinese(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}')
}

/// Punctuation that ends a clause; the next characters start a new one.
fn breaks(c: char) -> bool {
    matches!(c, '，' | '。' | '！' | '？' | '；' | '、' | '：' | '…')
}

/// Sounds people make while thinking. Each is dropped only at the start of a clause,
/// where it cannot be a sentence-final particle such as the 啊 in 好啊.
const HESITATIONS: [char; 7] = ['嗯', '呃', '欸', '啊', '喔', '噢', '唔'];
/// Characters a speaker often repeats when stumbling, and which are not doubled in ordinary words.
const STUMBLES: [char; 10] = ['我', '你', '他', '她', '它', '這', '那', '就', '要', '會'];
/// Two-character connectives that get said twice in a row.
const DOUBLED: [&str; 4] = ["然後", "就是", "那個", "這個"];

/// Recognisers without punctuation separate clauses with spaces, and some emit half-width marks.
fn repair_punctuation(text: &str) -> Vec<char> {
    let source: Vec<char> = text.trim().chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(source.len());
    let mut at = 0;
    while at < source.len() {
        let c = source[at];
        let before = out.last().copied();
        if c.is_whitespace() {
            let next = source[at..].iter().copied().find(|c| !c.is_whitespace());
            match (before, next) {
                (Some(left), Some(right)) if chinese(left) && chinese(right) => out.push('，'),
                (Some(left), Some(right)) if left.is_ascii_alphanumeric() && right.is_ascii_alphanumeric() => out.push(' '),
                _ => {}
            }
            while at < source.len() && source[at].is_whitespace() {
                at += 1;
            }
            continue;
        }
        let wide = match c {
            ',' => '，',
            '?' => '？',
            '!' => '！',
            ';' => '；',
            ':' => '：',
            // A full stop between digits or letters is a decimal point or part of a name.
            '.' if !source.get(at + 1).is_some_and(|next| next.is_ascii_alphanumeric()) => '。',
            _ => c,
        };
        out.push(if wide != c && before.is_some_and(chinese) { wide } else { c });
        at += 1;
    }
    out
}

fn drop_hesitations(chars: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(chars.len());
    let mut clause_start = true;
    let mut at = 0;
    while at < chars.len() {
        if clause_start && HESITATIONS.contains(&chars[at]) {
            // The sound, however long it is drawn out, and the pause mark after it.
            while at < chars.len() && (HESITATIONS.contains(&chars[at]) || matches!(chars[at], '，' | '、' | '…' | '～')) {
                at += 1;
            }
            continue;
        }
        clause_start = breaks(chars[at]);
        out.push(chars[at]);
        at += 1;
    }
    out
}

fn drop_stutters(chars: &[char]) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    for &c in chars {
        let repeated_stumble = STUMBLES.contains(&c) && out.last() == Some(&c);
        // Two clause marks in a row are left behind when a hesitation between them is removed.
        let doubled_mark = breaks(c) && out.last().is_some_and(|last| breaks(*last));
        if repeated_stumble {
            continue;
        }
        if doubled_mark {
            // The stronger mark wins: a comma gives way to whatever follows it.
            if matches!(out.last(), Some('，' | '、')) {
                *out.last_mut().unwrap() = c;
            }
            continue;
        }
        out.push(c);
        for pair in DOUBLED {
            let pair: Vec<char> = pair.chars().collect();
            let length = out.len();
            if length >= 4 && out[length - 4..length - 2] == pair[..] && out[length - 2..] == pair[..] {
                out.truncate(length - 2);
            }
        }
    }
    out
}

/// Returns the tidied text. Text with no Chinese in it is returned as it came.
pub fn tidy(text: &str) -> String {
    if !text.chars().any(chinese) {
        return text.trim().to_string();
    }
    let mut chars = drop_stutters(&drop_hesitations(&repair_punctuation(text)));
    while chars.first().is_some_and(|first| breaks(*first)) {
        chars.remove(0);
    }
    // A sentence of several clauses gets its full stop; a short phrase, such as a name or a search, does not.
    let clauses = chars.iter().any(|c| matches!(c, '，' | '；'));
    if clauses && chars.last().is_some_and(|last| chinese(*last) || last.is_ascii_alphanumeric()) {
        chars.push('。');
    }
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hesitation_sounds_at_the_start_of_a_clause_go() {
        assert_eq!(tidy("嗯，我想問一下，呃，明天開會嗎？"), "我想問一下，明天開會嗎？");
        assert_eq!(tidy("欸欸那個報告好了"), "那個報告好了");
        // Particles at the end of a clause are part of the sentence.
        assert_eq!(tidy("好啊，那就這樣喔。"), "好啊，那就這樣喔。");
    }

    #[test]
    fn stumbles_and_doubled_connectives_collapse() {
        assert_eq!(tidy("我我我覺得然後然後就是就是可以"), "我覺得然後就是可以");
        // Ordinary doubled words stay.
        assert_eq!(tidy("謝謝你，慢慢來，哈哈哈"), "謝謝你，慢慢來，哈哈哈。");
    }

    #[test]
    fn spaces_and_half_width_marks_become_chinese_punctuation() {
        assert_eq!(tidy("今天下午三點開會 請先準備報告"), "今天下午三點開會，請先準備報告。");
        assert_eq!(tidy("好的,沒問題!"), "好的，沒問題！");
        assert_eq!(tidy("版本是 3.5，用 GitHub Actions 跑"), "版本是3.5，用GitHub Actions跑。");
    }

    #[test]
    fn short_phrases_and_other_languages_are_left_alone() {
        assert_eq!(tidy(" 李曉明 "), "李曉明");
        assert_eq!(tidy("Hello, world."), "Hello, world.");
        assert_eq!(tidy("嗯"), "");
        assert_eq!(tidy(""), "");
    }
}
