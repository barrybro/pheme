//! Renders an article to a full HTML page, following NetNewsWire's ArticleRenderer.

use crate::store::ArticleDetail;
use crate::text::{escape_html, format_date, medium_datetime};
use crate::themes::Theme;

const TEMPLATE: &str = include_str!("../resources/article/template.html");
const STYLESHEET: &str = include_str!("../resources/article/stylesheet.css");

/// How pages look: the article theme (`None` for Pheme's own), light or dark, and
/// whether scripts may run.
pub struct Style {
    pub theme: Option<Theme>,
    pub dark: bool,
    pub javascript: bool,
}

impl Style {
    fn template(&self) -> &str {
        self.theme.as_ref().map_or(TEMPLATE, |t| &t.template)
    }

    /// QtWebEngine doesn't reliably follow the desktop's light/dark setting, so the
    /// stylesheet's color-scheme media queries are answered explicitly: on a screen,
    /// `(color)` always matches and `(monochrome)` never does.
    fn stylesheet(&self) -> String {
        let css = self.theme.as_ref().map_or(STYLESHEET, |t| &t.stylesheet);
        let (dark, light) = if self.dark { ("(color)", "(monochrome)") } else { ("(monochrome)", "(color)") };
        css.replace("(prefers-color-scheme: dark)", dark)
            .replace("(prefers-color-scheme:dark)", dark)
            .replace("(prefers-color-scheme: light)", light)
            .replace("(prefers-color-scheme:light)", light)
    }
}

/// `base` resolves the article's relative links and images, since the page itself is
/// loaded from a local file (see `write_page`).
fn page(body: &str, title: &str, style: &Style, base: Option<&str>) -> String {
    let css = style.stylesheet();
    let base = base
        .filter(|b| b.starts_with("http://") || b.starts_with("https://"))
        .map(|b| format!("<base href=\"{}\">", escape_html(b)))
        .unwrap_or_default();
    let csp = if style.javascript { "" } else { "<meta http-equiv=\"Content-Security-Policy\" content=\"script-src 'none'\">" };
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"UTF-8\">{base}\
         <meta name=\"viewport\" content=\"width=device-width\">{csp}\
         <title>{}</title><style>{css}</style></head><body>{body}</body></html>",
        escape_html(title)
    )
}

/// The placeholder shown when nothing (or several things) are selected.
pub fn system_message(message: &str, style: &Style) -> String {
    page(&format!("<div class=\"systemMessage\">{}</div>", escape_html(message)), "", style, None)
}

pub fn render(article: &ArticleDetail, text_size_class: &str, style: &Style) -> String {
    let title = article.title.clone().unwrap_or_default();
    let link = article.url.clone().unwrap_or_default();
    let byline = article
        .author
        .as_deref()
        .filter(|a| !a.trim().is_empty())
        .map(|a| format!("By {}", escape_html(a)))
        .unwrap_or_default();
    let avatar_src = article.icon_url.as_deref().map(escape_html).unwrap_or_default();
    let avatar = article
        .icon_url
        .as_deref()
        .map(|src| format!("<img id=\"nnwImageIcon\" src=\"{}\" height=48 width=48 />", escape_html(src)))
        .unwrap_or_default();
    let mut body = article.content_html.clone().unwrap_or_default();
    if body.trim().is_empty() {
        body = format!("<p><a href=\"{}\">Read on the web</a></p>", escape_html(&link));
    }

    let dateline_style = if title.is_empty() { "articleDatelineTitle" } else { "articleDateline" };
    let date = |format: &str| escape_html(&format_date(article.published, format));

    let substitutions: [(&str, String); 21] = [
        ("feed_link_title", escape_html(&article.feed_title)),
        ("feed_link", escape_html(article.feed_website.as_deref().unwrap_or(""))),
        ("byline", byline),
        ("avatar_src", avatar_src),
        ("avatar", avatar),
        ("preferred_link", escape_html(&link)),
        ("title", escape_html(&title)),
        ("dateline_style", dateline_style.into()),
        ("datetime_long", date("%B %-d, %Y at %-I:%M:%S %p")),
        ("datetime_medium", escape_html(&medium_datetime(article.published))),
        ("datetime_short", date("%-m/%-d/%y, %-I:%M %p")),
        ("date_long", date("%B %-d, %Y")),
        ("date_medium", date("%b %-d, %Y")),
        ("date_short", date("%-m/%-d/%y")),
        ("time_long", date("%-I:%M:%S %p %Z")),
        ("time_medium", date("%-I:%M:%S %p")),
        ("time_short", date("%-I:%M %p")),
        ("external_link_label", String::new()),
        ("external_link_stripped", String::new()),
        ("external_link", String::new()),
        ("text_size_class", text_size_class.into()),
    ];
    let mut html = style.template().to_string();
    for (key, value) in substitutions {
        html = html.replace(&format!("[[{key}]]"), &value);
    }
    // Body and enclosures last, so markup inside the article can't be mistaken for placeholders.
    // Like NetNewsWire, templates without [[enclosures]] get them after the body.
    if html.contains("[[enclosures]]") {
        html = html.replace("[[enclosures]]", &enclosures_html(article));
    } else {
        body = format!("<div>{body}</div>{}", enclosures_html(article));
    }
    html = html.replace("[[body]]", &body);
    page(&html, &title, style, article.url.as_deref())
}

fn enclosures_html(article: &ArticleDetail) -> String {
    let body = article.content_html.as_deref().unwrap_or("");
    let mut out = String::new();
    for (url, mime) in &article.enclosures {
        // Feedly often lists images that already appear in the body.
        if body.contains(url.as_str()) {
            continue;
        }
        let src = escape_html(url);
        match mime.as_deref().unwrap_or("") {
            m if m.starts_with("audio/") => out.push_str(&format!("<p><audio controls preload=\"none\" src=\"{src}\"></audio></p>")),
            m if m.starts_with("video/") => out.push_str(&format!("<p><video controls preload=\"none\" src=\"{src}\" style=\"max-width:100%\"></video></p>")),
            m if m.starts_with("image/") => out.push_str(&format!("<p><img src=\"{src}\"></p>")),
            _ => {}
        }
    }
    out
}

/// The origin of an article's web page, like `https://example.com/`: the Referer frames
/// embedded in the article get, as they would on the article's own site. YouTube's
/// player refuses to play (error 153) without one, and pages loaded from files send none.
pub fn embed_referrer(article_url: Option<&str>) -> Option<String> {
    let url = url::Url::parse(article_url?).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| format!("{}/", url.origin().ascii_serialization()))
}

/// Writes a rendered page to a private runtime file and returns a URL for it. The URL
/// carries `referrer` (see `embed_referrer`) for the request interceptor in platform.cpp.
///
/// The article view loads pages by URL rather than with `loadHtml`: QtWebEngine's
/// `loadHtml` sometimes commits a blank page instead of the HTML it was given.
/// Two files alternate so each load is a real navigation to a different URL.
pub fn write_page(html: &str, referrer: Option<&str>) -> anyhow::Result<String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("pheme");
    std::fs::create_dir_all(&dir)?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("article-{}.html", n % 2));
    std::fs::write(&path, html)?;
    let mut url = url::Url::from_file_path(&path).map_err(|_| anyhow::anyhow!("bad path"))?;
    url.query_pairs_mut().append_pair("v", &n.to_string());
    if let Some(referrer) = referrer {
        url.query_pairs_mut().append_pair("referrer", referrer);
    }
    Ok(url.into())
}
