//! A deliberately small shell splitter.
//!
//! The policy engine only needs to know *which* programs a command line runs
//! and *with which words* — enough to grade `rm -rf /` as fatal while letting
//! `rm ./build.log` through. Building a fake parser is cheaper than pulling a
//! full grammar, and it keeps the crate dependency-free.
//!
//! Covered: single and double quotes, backslash escapes, `;` `&&` `||` `|`
//! `&` separators, subshell parentheses, output redirections, comments, and
//! commands nested in `$(…)`, backticks or `sh -c "…"`.
//!
//! Not covered (and not needed): here-documents, arithmetic `(( ))`, brace
//! expansion. Those degrade to plain words, which is the safe direction — an
//! unparsed token is graded by the words it contains.

/// A whole command line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    pub segments: Vec<Segment>,
}

impl Command {
    /// Every segment including those nested in substitutions, depth first.
    pub fn flatten(&self) -> Vec<&Segment> {
        let mut out = Vec::new();
        for segment in &self.segments {
            collect(segment, &mut out);
        }
        out
    }

    /// The line with redundant whitespace removed, quotes kept. Used for rule
    /// matching so a pattern does not have to guess the user's spacing.
    pub fn normalized(&self) -> String {
        let raw: Vec<&str> = self.segments.iter().map(|s| s.raw.as_str()).collect();
        collapse(&raw.join(" ; "))
    }
}

fn collect<'a>(segment: &'a Segment, out: &mut Vec<&'a Segment>) {
    out.push(segment);
    for nested in &segment.nested {
        collect(nested, out);
    }
}

/// One simple command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Segment {
    /// Words with quotes removed and escapes resolved; `argv[0]` is the program.
    pub argv: Vec<String>,
    /// The segment as typed, separators trimmed, quotes retained.
    pub raw: String,
    pub redirects: Vec<Redirect>,
    /// Commands hidden in `$(…)`, backticks or `sh -c "…"`.
    pub nested: Vec<Segment>,
}

/// An output redirection target, which is where a write can land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub target: String,
    pub append: bool,
}

impl Segment {
    /// The program name without a directory prefix (`/bin/rm` → `rm`).
    pub fn program(&self) -> &str {
        let first = self.argv.first().map(String::as_str).unwrap_or("");
        first.rsplit('/').next().unwrap_or(first)
    }

    pub fn rest(&self) -> &[String] {
        self.argv.get(1..).unwrap_or(&[])
    }

    /// `true` when any word is exactly `flag` or a short cluster containing it.
    pub fn has_flag(&self, flag: &str) -> bool {
        self.rest().iter().any(|word| {
            word == flag
                || (flag.len() == 2
                    && flag.starts_with('-')
                    && word.starts_with('-')
                    && !word.starts_with("--")
                    && word[1..].contains(&flag[1..]))
        })
    }

    pub fn has_any_flag(&self, flags: &[&str]) -> bool {
        flags.iter().any(|flag| self.has_flag(flag))
    }
}

/// Collapse runs of whitespace outside quotes.
pub fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut space = false;
    for ch in text.chars() {
        match quote {
            Some(q) => {
                out.push(ch);
                if ch == q {
                    quote = None;
                }
            }
            None => {
                if ch == '\'' || ch == '"' {
                    if space && !out.is_empty() {
                        out.push(' ');
                    }
                    space = false;
                    quote = Some(ch);
                    out.push(ch);
                } else if ch.is_whitespace() {
                    space = true;
                } else {
                    if space && !out.is_empty() {
                        out.push(' ');
                    }
                    space = false;
                    out.push(ch);
                }
            }
        }
    }
    out
}

pub fn parse(input: &str) -> Command {
    Command {
        segments: split(input),
    }
}

fn split(input: &str) -> Vec<Segment> {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<Segment> = Vec::new();
    let mut segment = Segment::default();
    let mut word = String::new();
    let mut raw = String::new();
    let mut index = 0usize;

    macro_rules! flush_word {
        () => {
            if !word.is_empty() {
                segment.argv.push(std::mem::take(&mut word));
            }
        };
    }
    macro_rules! end_segment {
        () => {
            flush_word!();
            if !segment.argv.is_empty()
                || !segment.redirects.is_empty()
                || !segment.nested.is_empty()
            {
                segment.raw = raw
                    .trim()
                    .trim_matches(|ch: char| matches!(ch, ';' | '&' | '|' | '(' | ')' | '\n'))
                    .trim()
                    .to_string();
                out.push(std::mem::take(&mut segment));
            }
            raw.clear();
        };
    }

    while index < chars.len() {
        let ch = chars[index];
        match ch {
            ' ' | '\t' => {
                flush_word!();
                raw.push(ch);
                index += 1;
            }
            '\n' | ';' => {
                raw.push(ch);
                index += 1;
                end_segment!();
            }
            '&' => {
                raw.push(ch);
                index += 1;
                if index < chars.len() && chars[index] == '&' {
                    raw.push('&');
                    index += 1;
                }
                end_segment!();
            }
            '|' => {
                raw.push(ch);
                index += 1;
                if index < chars.len() && chars[index] == '|' {
                    raw.push('|');
                    index += 1;
                }
                end_segment!();
            }
            '(' | ')' => {
                raw.push(ch);
                index += 1;
                end_segment!();
            }
            '#' if word.is_empty() => {
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
            }
            '\'' => {
                raw.push(ch);
                index += 1;
                while index < chars.len() && chars[index] != '\'' {
                    raw.push(chars[index]);
                    word.push(chars[index]);
                    index += 1;
                }
                if index < chars.len() {
                    raw.push('\'');
                    index += 1;
                }
            }
            '"' => {
                raw.push(ch);
                index += 1;
                while index < chars.len() && chars[index] != '"' {
                    let inner = chars[index];
                    if inner == '\\' && index + 1 < chars.len() {
                        let escaped = chars[index + 1];
                        raw.push(inner);
                        raw.push(escaped);
                        if matches!(escaped, '"' | '\\' | '$' | '`') {
                            word.push(escaped);
                        } else {
                            word.push('\\');
                            word.push(escaped);
                        }
                        index += 2;
                        continue;
                    }
                    if inner == '$' && index + 1 < chars.len() && chars[index + 1] == '(' {
                        let (body, next) = take_paren(&chars, index);
                        raw.extend(&chars[index..next]);
                        segment.nested.extend(split(&body));
                        index = next;
                        continue;
                    }
                    if inner == '`' {
                        let (body, next) = take_backtick(&chars, index);
                        raw.extend(&chars[index..next]);
                        segment.nested.extend(split(&body));
                        index = next;
                        continue;
                    }
                    raw.push(inner);
                    word.push(inner);
                    index += 1;
                }
                if index < chars.len() {
                    raw.push('"');
                    index += 1;
                }
            }
            '\\' => {
                raw.push(ch);
                index += 1;
                if index < chars.len() {
                    raw.push(chars[index]);
                    word.push(chars[index]);
                    index += 1;
                }
            }
            '$' if index + 1 < chars.len() && chars[index + 1] == '(' => {
                let (body, next) = take_paren(&chars, index);
                raw.extend(&chars[index..next]);
                segment.nested.extend(split(&body));
                index = next;
            }
            '`' => {
                let (body, next) = take_backtick(&chars, index);
                raw.extend(&chars[index..next]);
                segment.nested.extend(split(&body));
                index = next;
            }
            '>' => {
                // `2>` and friends: the file-descriptor digits are not a word.
                if !word.is_empty() && word.chars().all(|ch| ch.is_ascii_digit()) {
                    word.clear();
                }
                raw.push(ch);
                index += 1;
                let mut append = false;
                if index < chars.len() && chars[index] == '>' {
                    append = true;
                    raw.push('>');
                    index += 1;
                }
                // `2>&1`, `>&2`, `2>&-`: a descriptor copy, not a file target
                // and not a command separator. Left alone, the `&` would split
                // the line and `1` would be graded as an unknown program.
                if index < chars.len() && chars[index] == '&' {
                    raw.push('&');
                    index += 1;
                    while index < chars.len()
                        && (chars[index].is_ascii_digit() || chars[index] == '-')
                    {
                        raw.push(chars[index]);
                        index += 1;
                    }
                    continue;
                }
                while index < chars.len() && (chars[index] == ' ' || chars[index] == '\t') {
                    raw.push(chars[index]);
                    index += 1;
                }
                let mut target = String::new();
                while index < chars.len() {
                    let ch = chars[index];
                    if ch.is_whitespace()
                        || matches!(ch, ';' | '|' | '>' | '<' | '(' | ')')
                        || ch == '&'
                    {
                        break;
                    }
                    raw.push(ch);
                    if ch == '"' || ch == '\'' {
                        let quote = ch;
                        index += 1;
                        while index < chars.len() && chars[index] != quote {
                            raw.push(chars[index]);
                            target.push(chars[index]);
                            index += 1;
                        }
                        if index < chars.len() {
                            raw.push(quote);
                            index += 1;
                        }
                        continue;
                    }
                    target.push(ch);
                    index += 1;
                }
                if !target.is_empty() {
                    segment.redirects.push(Redirect { target, append });
                }
            }
            '<' => {
                raw.push(ch);
                index += 1;
            }
            _ => {
                raw.push(ch);
                word.push(ch);
                index += 1;
            }
        }
    }
    end_segment!();
    out
}

/// Body of `$( … )`, starting at the `$`. Returns the body and the index just
/// past the closing parenthesis.
fn take_paren(chars: &[char], start: usize) -> (String, usize) {
    let mut depth = 1usize;
    let mut index = start + 2;
    let mut body = String::new();
    let mut quote: Option<char> = None;
    while index < chars.len() {
        let ch = chars[index];
        if let Some(active) = quote {
            if ch == '\\' && active == '"' && index + 1 < chars.len() {
                body.push(ch);
                body.push(chars[index + 1]);
                index += 2;
                continue;
            }
            if ch == active {
                quote = None;
            }
            body.push(ch);
            index += 1;
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                body.push(ch);
                index += 1;
            }
            '(' => {
                depth += 1;
                body.push(ch);
                index += 1;
            }
            ')' => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return (body, index);
                }
                body.push(ch);
            }
            _ => {
                body.push(ch);
                index += 1;
            }
        }
    }
    (body, index)
}

fn take_backtick(chars: &[char], start: usize) -> (String, usize) {
    let mut index = start + 1;
    let mut body = String::new();
    while index < chars.len() {
        let ch = chars[index];
        if ch == '`' {
            return (body, index + 1);
        }
        if ch == '\\' && index + 1 < chars.len() {
            body.push(ch);
            body.push(chars[index + 1]);
            index += 2;
            continue;
        }
        body.push(ch);
        index += 1;
    }
    (body, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(input: &str) -> Vec<Vec<String>> {
        parse(input)
            .segments
            .iter()
            .map(|segment| segment.argv.clone())
            .collect()
    }

    #[test]
    fn splits_pipelines_and_lists() {
        let command = parse("ls -la | grep foo && echo ok; pwd");
        let programs: Vec<&str> = command.segments.iter().map(Segment::program).collect();
        assert_eq!(programs, vec!["ls", "grep", "echo", "pwd"]);
        assert_eq!(command.segments[1].argv, vec!["grep", "foo"]);
    }

    #[test]
    fn keeps_quoted_words_whole() {
        assert_eq!(
            argv(r#"printf 'a | b' "c && d""#),
            vec![vec!["printf", "a | b", "c && d"]]
        );
    }

    #[test]
    fn resolves_escapes_outside_quotes() {
        assert_eq!(argv(r"echo a\ b"), vec![vec!["echo", "a b"]]);
    }

    #[test]
    fn handles_a_cd_prefixed_delete() {
        let command = parse("cd /tmp && rm -rf build");
        assert_eq!(command.segments.len(), 2);
        assert_eq!(command.segments[1].program(), "rm");
        assert!(command.segments[1].has_flag("-rf"));
    }

    #[test]
    fn strips_a_directory_prefix() {
        assert_eq!(parse("/bin/rm -rf /").segments[0].program(), "rm");
    }

    #[test]
    fn records_output_redirections() {
        let command = parse("echo x > /etc/motd");
        let redirect = &command.segments[0].redirects[0];
        assert_eq!(redirect.target, "/etc/motd");
        assert!(!redirect.append);
        assert_eq!(argv("echo x >> log")[0], vec!["echo", "x"]);
    }

    #[test]
    fn ignores_file_descriptor_prefixes() {
        let command = parse("cmd 2> err.log");
        assert_eq!(command.segments[0].argv, vec!["cmd"]);
        assert_eq!(command.segments[0].redirects[0].target, "err.log");
    }

    #[test]
    fn descriptor_copies_are_not_separators() {
        // `2>&1` used to split the line, leaving `1` as an unknown program and
        // pushing a read-only command up to R2.
        let command = parse("smartctl -H /dev/sda 2>&1");
        assert_eq!(command.segments.len(), 1);
        assert_eq!(
            command.segments[0].argv,
            vec!["smartctl", "-H", "/dev/sda"]
        );
        assert!(command.segments[0].redirects.is_empty());
        assert_eq!(parse("cmd >&2").segments[0].program(), "cmd");
        assert_eq!(parse("cmd 2>&-").segments.len(), 1);
    }

    #[test]
    fn descends_into_substitutions() {
        let command = parse("echo $(rm -rf /tmp/x)");
        let nested = command.segments[0].nested.clone();
        assert_eq!(nested[0].program(), "rm");
        assert_eq!(command.flatten().len(), 2);
    }

    #[test]
    fn descends_into_backticks_inside_quotes() {
        let command = parse("tar -czf a.tgz \"`rm -rf build`\"");
        assert_eq!(command.segments[0].nested[0].program(), "rm");
    }

    #[test]
    fn descends_into_nested_parens() {
        let command = parse("echo $(echo $(rm -rf x))");
        assert_eq!(command.flatten().len(), 3);
    }

    #[test]
    fn skips_comments() {
        assert_eq!(argv("ls # rm -rf /"), vec![vec!["ls"]]);
    }

    #[test]
    fn splits_subshell_parentheses() {
        let command = parse("(cd /tmp; rm x)");
        let programs: Vec<&str> = command.segments.iter().map(Segment::program).collect();
        assert_eq!(programs, vec!["cd", "rm"]);
    }

    #[test]
    fn collapse_keeps_quoted_spaces() {
        assert_eq!(collapse("a   'b   c'"), "a 'b   c'");
    }

    #[test]
    fn detects_short_flag_clusters() {
        let command = parse("rm -rfv /tmp");
        assert!(command.segments[0].has_flag("-r"));
        assert!(command.segments[0].has_flag("-f"));
        assert!(!command.segments[0].has_flag("-x"));
    }
}
