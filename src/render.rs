//! Renders an article to a full HTML page, following NetNewsWire's ArticleRenderer.

use crate::store::ArticleDetail;
use crate::text::{escape_html, medium_datetime};

const TEMPLATE: &str = include_str!("../resources/article/template.html");
const STYLESHEET: &str = include_str!("../resources/article/stylesheet.css");

/// QtWebEngine doesn't reliably follow the desktop's light/dark setting, so the
/// stylesheet's dark-mode media queries are switched on or off explicitly.
fn stylesheet(dark: bool) -> String {
    let replacement = if dark { "@media all" } else { "@media not all" };
    STYLESHEET
        .replace("@media(prefers-color-scheme: dark)", replacement)
        .replace("@media screen and (prefers-color-scheme: dark)", replacement)
}

/// `base` resolves the article's relative links and images, since the page itself is
/// loaded from a local file (see `write_page`).
fn page(body: &str, title: &str, dark: bool, base: Option<&str>) -> String {
    let style = stylesheet(dark);
    let base = base
        .filter(|b| b.starts_with("http://") || b.starts_with("https://"))
        .map(|b| format!("<base href=\"{}\">", escape_html(b)))
        .unwrap_or_default();
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"UTF-8\">{base}\
         <meta name=\"viewport\" content=\"width=device-width\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"script-src 'none'\">\
         <title>{}</title><style>{style}</style></head><body>{body}</body></html>",
        escape_html(title)
    )
}

/// The placeholder shown when nothing (or several things) are selected.
pub fn system_message(message: &str, dark: bool) -> String {
    page(&format!("<div class=\"systemMessage\">{}</div>", escape_html(message)), "", dark, None)
}

pub fn render(article: &ArticleDetail, text_size_class: &str, dark: bool) -> String {
    let title = article.title.clone().unwrap_or_default();
    let link = article.url.clone().unwrap_or_default();
    let byline = article
        .author
        .as_deref()
        .filter(|a| !a.trim().is_empty())
        .map(|a| format!("By {}", escape_html(a)))
        .unwrap_or_default();
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

    let substitutions: [(&str, String); 12] = [
        ("feed_link_title", escape_html(&article.feed_title)),
        ("feed_link", escape_html(article.feed_website.as_deref().unwrap_or(""))),
        ("byline", byline),
        ("avatar", avatar),
        ("preferred_link", escape_html(&link)),
        ("title", escape_html(&title)),
        ("dateline_style", dateline_style.into()),
        ("datetime_medium", escape_html(&medium_datetime(article.published))),
        ("external_link_label", String::new()),
        ("external_link_stripped", String::new()),
        ("external_link", String::new()),
        ("text_size_class", text_size_class.into()),
    ];
    let mut html = TEMPLATE.to_string();
    for (key, value) in substitutions {
        html = html.replace(&format!("[[{key}]]"), &value);
    }
    // Body and enclosures last, so markup inside the article can't be mistaken for placeholders.
    html = html.replace("[[enclosures]]", &enclosures_html(article));
    html = html.replace("[[body]]", &body);
    page(&html, &title, dark, article.url.as_deref())
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

/// Writes a rendered page to a private runtime file and returns a URL for it.
///
/// The article view loads pages by URL rather than with `loadHtml`: QtWebEngine's
/// `loadHtml` sometimes commits a blank page instead of the HTML it was given.
/// Two files alternate so each load is a real navigation to a different URL.
pub fn write_page(html: &str) -> anyhow::Result<String> {
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
    Ok(format!("{}?v={n}", url::Url::from_file_path(&path).map_err(|_| anyhow::anyhow!("bad path"))?))
}
