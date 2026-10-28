//! Small text helpers: HTML to plain text for timeline summaries, escaping, and dates.

use chrono::{DateTime, Local, TimeZone};

/// Strips tags, decodes common entities, and collapses whitespace, stopping after `max_chars`.
pub fn html_to_text(html: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(html.len().min(max_chars * 2));
    let mut chars = html.chars().peekable();
    let mut in_tag = false;
    let mut skip_until: Option<&str> = None;
    let mut tag = String::new();
    let mut last_space = true;
    let mut count = 0;

    let push = |out: &mut String, c: char, last_space: &mut bool, count: &mut usize| {
        if c.is_whitespace() {
            if !*last_space {
                out.push(' ');
                *count += 1;
                *last_space = true;
            }
        } else {
            out.push(c);
            *count += 1;
            *last_space = false;
        }
    };

    while let Some(c) = chars.next() {
        if count >= max_chars {
            break;
        }
        if in_tag {
            if c == '>' {
                in_tag = false;
                let name: String = tag
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                let closing = tag.starts_with('/');
                if let Some(end) = skip_until {
                    if closing && name == end {
                        skip_until = None;
                    }
                } else if !closing && (name == "script" || name == "style") {
                    skip_until = Some(if name == "script" { "script" } else { "style" });
                } else if matches!(name.as_str(), "p" | "br" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "tr" | "blockquote") {
                    push(&mut out, ' ', &mut last_space, &mut count);
                }
                tag.clear();
            } else {
                tag.push(c);
            }
            continue;
        }
        if c == '<' {
            in_tag = true;
            continue;
        }
        if skip_until.is_some() {
            continue;
        }
        if c == '&' {
            let mut entity = String::new();
            while let Some(&n) = chars.peek() {
                if n == ';' || entity.len() > 10 || n.is_whitespace() || n == '<' {
                    break;
                }
                entity.push(n);
                chars.next();
            }
            let terminated = chars.peek() == Some(&';');
            if terminated {
                chars.next();
                if let Some(decoded) = decode_entity(&entity) {
                    push(&mut out, decoded, &mut last_space, &mut count);
                    continue;
                }
            }
            // Not an entity we know; keep the text as written.
            for c in std::iter::once('&').chain(entity.chars()).chain(terminated.then_some(';')) {
                push(&mut out, c, &mut last_space, &mut count);
            }
            continue;
        }
        push(&mut out, c, &mut last_space, &mut count);
    }
    out.trim().to_string()
}

/// Decodes HTML character references, leaving everything else, tags and unknown entities
/// included, as written. Titles are plain text, but some feeds put HTML entities that XML
/// doesn't define in them, like `&times;`, or escape entities twice, like `&amp;#8217;`.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| Some((decode_entity(&rest[..end])?, end + 1)));
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => out.push('&'),
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    if let Some(num) = entity.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match entity {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "times" => '×',
        "divide" => '÷',
        "plusmn" => '±',
        "minus" => '−',
        "deg" => '°',
        "middot" => '·',
        "bull" => '•',
        "laquo" => '«',
        "raquo" => '»',
        "lsaquo" => '‹',
        "rsaquo" => '›',
        "sbquo" => '‚',
        "bdquo" => '„',
        "prime" => '′',
        "Prime" => '″',
        "larr" => '←',
        "rarr" => '→',
        "uarr" => '↑',
        "darr" => '↓',
        "harr" => '↔',
        "euro" => '€',
        "pound" => '£',
        "cent" => '¢',
        "yen" => '¥',
        "sect" => '§',
        "para" => '¶',
        "micro" => 'µ',
        "frac12" => '½',
        "frac14" => '¼',
        "frac34" => '¾',
        "sup2" => '²',
        "sup3" => '³',
        "iexcl" => '¡',
        "iquest" => '¿',
        "aacute" => 'á',
        "agrave" => 'à',
        "acirc" => 'â',
        "atilde" => 'ã',
        "auml" => 'ä',
        "aring" => 'å',
        "aelig" => 'æ',
        "ccedil" => 'ç',
        "eacute" => 'é',
        "egrave" => 'è',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "iacute" => 'í',
        "igrave" => 'ì',
        "icirc" => 'î',
        "iuml" => 'ï',
        "ntilde" => 'ñ',
        "oacute" => 'ó',
        "ograve" => 'ò',
        "ocirc" => 'ô',
        "otilde" => 'õ',
        "ouml" => 'ö',
        "oslash" => 'ø',
        "szlig" => 'ß',
        "uacute" => 'ú',
        "ugrave" => 'ù',
        "ucirc" => 'û',
        "uuml" => 'ü',
        "yacute" => 'ý',
        "yuml" => 'ÿ',
        "Aacute" => 'Á',
        "Agrave" => 'À',
        "Auml" => 'Ä',
        "Aring" => 'Å',
        "Ccedil" => 'Ç',
        "Eacute" => 'É',
        "Egrave" => 'È',
        "Ntilde" => 'Ñ',
        "Oacute" => 'Ó',
        "Ouml" => 'Ö',
        "Oslash" => 'Ø',
        "Uacute" => 'Ú',
        "Uuml" => 'Ü',
        _ => return None,
    })
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn local(ms: i64) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(ms).single()
}

/// The timeline's date column: a time for today, a weekday this week, otherwise a short date.
pub fn timeline_date(ms: i64) -> String {
    let Some(dt) = local(ms) else { return String::new() };
    let now = Local::now();
    let days = (now.date_naive() - dt.date_naive()).num_days();
    if days == 0 {
        dt.format("%-I:%M %p").to_string()
    } else if (1..7).contains(&days) {
        dt.format("%A").to_string()
    } else {
        dt.format("%b %-d, %Y").to_string()
    }
}

/// The article view's dateline, e.g. "Oct 27, 2026 at 6:53 PM".
pub fn medium_datetime(ms: i64) -> String {
    format_date(ms, "%b %-d, %Y at %-I:%M %p")
}

/// Formats a timestamp in local time with a chrono format string.
pub fn format_date(ms: i64, format: &str) -> String {
    local(ms).map(|dt| dt.format(format).to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_entities_in_titles() {
        assert_eq!(decode_entities("1024 &times; 1024"), "1024 × 1024");
        assert_eq!(decode_entities("Here&#8217;s What&#x2019;s New"), "Here’s What’s New");
        assert_eq!(decode_entities("AT&T & <b>bold</b> &bogus; &amp"), "AT&T & <b>bold</b> &bogus; &amp");
        assert_eq!(decode_entities("café &eacute;t&eacute;"), "café été");
    }

    #[test]
    fn strips_markup_and_decodes_entities() {
        assert_eq!(html_to_text("<p>Hi&nbsp;<b>there</b> &amp; &#8220;you&#x201D;</p><p>Next</p>", 100), "Hi there & “you” Next");
        assert_eq!(html_to_text("<style>p{}</style>Body<script>x()</script>", 100), "Body");
        assert_eq!(html_to_text("AT&T &bogus; ok", 100), "AT&T &bogus; ok");
        assert_eq!(html_to_text("abcdef", 3), "abc");
    }
}
