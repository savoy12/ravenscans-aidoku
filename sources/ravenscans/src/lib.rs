// RavenScans Aidoku source (en, NSFW)
// id: com.ravenscans
// NOTE: This is a minimal, clean implementation meant to pass Aidoku source expectations.
// You can tune selectors if the site changes.

#![allow(unused)]
use aidoku::{
    error::Result,
    prelude::*,
    std::{html::Node, json, net, String, Vec},
    Chapter, Filter, FilterType, Listing, Manga, MangaPageResult, MangaStatus, MangaContentRating,
    MangaViewer, Page, Source
};
use once_cell::sync::Lazy;
use tl::ParserOptions;

// ------- Config -------
static BASE_URL: &str = "https://ravenscans.com";
static UA: Lazy<String> = Lazy::new(|| {
    "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Aidoku".into()
});

// Helper: GET and parse HTML
fn get_dom(url: &str) -> Result<tl::VDom> {
    let data = net::http_get(url, Some(&[("User-Agent", &UA)])).expect("http get failed");
    let html = String::from_utf8_lossy(&data).to_string();
    let parser = tl::parse(&html, ParserOptions::default()).expect("parse failed");
    Ok(parser)
}

fn text(node: &Node) -> String {
    node.inner_text().trim().to_string()
}

fn abs(href: &str) -> String {
    if href.starts_with("http") { href.to_string() } else { format!("{BASE_URL}{}", href) }
}

// Selectors (Madara-like; tweak if site changes)
mod sel {
    pub const LIST_ITEM: &str = "div.page-item-detail, div.col-6.col-md-3 div.item, div.bsx"; // fallback combos
    pub const TITLE: &str = "h3 a, .post-title a, .tt";
    pub const COVER: &str = "img";
    pub const HREF: &str = "a";
    pub const MANGA_META: &str = "div.post-content, .infox";
    pub const SUMMARY: &str = ".summary__content, .entry-content, .desc";
    pub const GENRES: &str = ".genres a, .wd-full .mgen a";
    pub const STATUS: &str = ".post-status .summary-content, .imptdt:contains(Status) i, .tsinfo .imptdt:nth-child(2) i";
    pub const CHAPTER_LIST: &str = "li.wp-manga-chapter, ul.main .lch a, .cl li a, .eplister ul li a";
    pub const CHAPTER_DATE: &str = "span.chapter-release-date, .chapter-time, .right i";
    pub const PAGE_IMAGE: &str = "div.reading-content img, .entry-content img, .read-content img";
    pub const PAGINATION_NEXT: &str = "a.next, a.r, a.nav-previous";
    pub const POPULAR_BLOCK: &str = ".popular-items, .serieslist.popular";
    pub const LATEST_BLOCK:  &str = ".c-tabs-item__content, .listupd";
}

// Map common status strings
fn map_status(s: &str) -> MangaStatus {
    let s = s.to_lowercase();
    if s.contains("ongoing") { MangaStatus::Ongoing }
    else if s.contains("completed") || s.contains("complete") { MangaStatus::Completed }
    else { MangaStatus::Unknown }
}

fn extract_cover(node: &Node) -> Option<String> {
    // tries data-src/src/srcset
    let img = node.query_selector(sel::COVER).ok()?.next()?;
    let attrs = img.as_tag()?.attributes();
    if let Some(v) = attrs.get("data-src").and_then(|a| a.get(0)) {
        return Some(v.as_utf8_str().to_string());
    }
    if let Some(v) = attrs.get("src").and_then(|a| a.get(0)) {
        return Some(v.as_utf8_str().to_string());
    }
    if let Some(v) = attrs.get("data-lazy-src").and_then(|a| a.get(0)) {
        return Some(v.as_utf8_str().to_string());
    }
    None
}

// ---- Source impl ----
#[get_manga_list]
fn get_manga_list(filters: Vec<Filter>, page: i32) -> Result<MangaPageResult> {
    // We implement two listings: "Latest" (default) and "Popular".
    // Aidoku passes a Listing filter; if not present, we default to Latest.
    let mut listing = "Latest";
    for f in &filters {
        if let Filter::Title { value } = f {
            if value == "Popular" { listing = "Popular"; }
        }
    }

    let url = match listing {
        "Popular" => format!("{BASE_URL}/?s=&post_type=wp-manga&m_orderby=trending"),
        _         => format!("{BASE_URL}/?s=&post_type=wp-manga&m_orderby=latest"),
    } + &format!("&page={}", if page < 1 { 1 } else { page });

    let dom = get_dom(&url)?;
    let mut mangas: Vec<Manga> = Vec::new();

    for item in dom.query_selector(sel::LIST_ITEM).unwrap_or_default() {
        let title_node = item.query_selector(sel::TITLE).ok().and_then(|mut q| q.next());
        let title = title_node.as_ref().map(text).unwrap_or_default();

        let href_node = item.query_selector(sel::HREF).ok().and_then(|mut q| q.next());
        let href = href_node
            .and_then(|n| n.as_tag()?.attributes().get("href").and_then(|a| a.get(0)))
            .map(|v| v.as_utf8_str().to_string())
            .unwrap_or_default();

        if title.is_empty() || href.is_empty() { continue; }

        let cover = extract_cover(&item);
        mangas.push(Manga {
            id: href.clone(),
            cover: cover.unwrap_or_default(),
            title,
            author: String::new(),
            artist: String::new(),
            description: String::new(),
            url: href,
            categories: Vec::new(),
            status: MangaStatus::Unknown,
            nsfw: MangaContentRating::Nsfw,
            viewer: MangaViewer::Scroll, // typical for webtoon/manhua
        });
    }

    // Basic "has_more" heuristic (Madara has page query; we assume true if many items)
    Ok(MangaPageResult {
        manga: mangas,
        has_more: true,
    })
}

#[get_manga_details]
fn get_manga_details(id: String) -> Result<Manga> {
    let dom = get_dom(&id)?;
    let info = dom.query_selector(sel::MANGA_META).ok().and_then(|mut q| q.next());

    // Title
    let title = dom
        .query_selector("h1, .entry-title, .post-title h1")
        .ok().and_then(|mut q| q.next())
        .map(text)
        .unwrap_or_else(|| "Unknown".into());

    // Description
    let description = info
        .as_ref()
        .and_then(|n| n.query_selector(sel::SUMMARY).ok()?.next())
        .map(text)
        .unwrap_or_default();

    // Genres
    let mut genres = Vec::new();
    if let Some(meta) = &info {
        for g in meta.query_selector(sel::GENRES).unwrap_or_default() {
            let t = text(&g);
            if !t.is_empty() { genres.push(t); }
        }
    }

    // Status
    let status = info
        .as_ref()
        .and_then(|n| n.query_selector(sel::STATUS).ok()?.next())
        .map(|n| map_status(&text(&n)))
        .unwrap_or(MangaStatus::Unknown);

    // Cover (try og:image)
    let cover = dom
        .query_selector("meta[property='og:image']")
        .ok().and_then(|mut q| q.next())
        .and_then(|m| m.as_tag()?.attributes().get("content").and_then(|a| a.get(0)))
        .map(|v| v.as_utf8_str().to_string())
        .unwrap_or_default();

    Ok(Manga {
        id: id.clone(),
        cover,
        title,
        author: String::new(),
        artist: String::new(),
        description,
        url: id,
        categories: genres,
        status,
        nsfw: MangaContentRating::Nsfw,
        viewer: MangaViewer::Scroll,
    })
}

#[get_chapter_list]
fn get_chapter_list(id: String) -> Result<Vec<Chapter>> {
    let dom = get_dom(&id)?;
    let mut chapters: Vec<Chapter> = Vec::new();

    for a in dom.query_selector(sel::CHAPTER_LIST).unwrap_or_default() {
        let link = a
            .as_tag().and_then(|t| t.attributes().get("href").and_then(|v| v.get(0)))
            .map(|v| v.as_utf8_str().to_string());

        let name = text(&a);
        if let Some(href) = link {
            // date (best-effort)
            let date_str = a.query_selector(sel::CHAPTER_DATE).ok().and_then(|mut q| q.next()).map(text);
            let date_updated = None::<f64>; // Keep None; Aidoku can accept unknown

            chapters.push(Chapter {
                id: href.clone(),
                title: name,
                volume: String::new(),
                chapter: String::new(),
                url: href,
                date_updated,
                scanlator: String::new(),
                lang: String::from("en"),
            });
        }
    }

    // Madara lists newest first; Aidoku expects newest first too, so we keep order.
    Ok(chapters)
}

#[get_page_list]
fn get_page_list(id: String) -> Result<Vec<Page>> {
    let dom = get_dom(&id)?;
    let mut pages: Vec<Page> = Vec::new();
    let mut index = 0;

    for img in dom.query_selector(sel::PAGE_IMAGE).unwrap_or_default() {
        if let Some(tag) = img.as_tag() {
            let attrs = tag.attributes();
            // Prefer data-src / src
            let url = attrs
                .get("data-src").and_then(|v| v.get(0))
                .or_else(|| attrs.get("src").and_then(|v| v.get(0)))
                .map(|v| v.as_utf8_str().to_string());

            if let Some(u) = url {
                pages.push(Page {
                    index,
                    url: u,
                    base64: String::new(),
                    text: String::new(),
                });
                index += 1;
            }
        }
    }

    Ok(pages)
}

#[get_search_results]
fn get_search_results(filters: Vec<Filter>, page: i32) -> Result<MangaPageResult> {
    // Use WP search: /?s=term&post_type=wp-manga
    let mut query = String::new();
    for f in filters {
        match f {
            Filter::Title { value } => { query = value; }
            Filter::Genre(_) | Filter::Select{..} | Filter::Sort {..} | Filter::Check {..} | Filter::Group{..} => {}
            _ => {}
        }
    }
    let p = if page < 1 { 1 } else { page };
    let url = format!("{BASE_URL}/?s={}&post_type=wp-manga&page={}", net::urlencode(&query), p);
    let dom = get_dom(&url)?;

    let mut mangas: Vec<Manga> = Vec::new();
    for item in dom.query_selector(sel::LIST_ITEM).unwrap_or_default() {
        let title_node = item.query_selector(sel::TITLE).ok().and_then(|mut q| q.next());
        let title = title_node.as_ref().map(text).unwrap_or_default();

        let href_node = item.query_selector(sel::HREF).ok().and_then(|mut q| q.next());
        let href = href_node
            .and_then(|n| n.as_tag()?.attributes().get("href").and_then(|a| a.get(0)))
            .map(|v| v.as_utf8_str().to_string())
            .unwrap_or_default();

        if title.is_empty() || href.is_empty() { continue; }
        let cover = super::extract_cover(&item);

        mangas.push(Manga {
            id: href.clone(),
            cover: cover.unwrap_or_default(),
            title,
            author: String::new(),
            artist: String::new(),
            description: String::new(),
            url: href,
            categories: Vec::new(),
            status: MangaStatus::Unknown,
            nsfw: MangaContentRating::Nsfw,
            viewer: MangaViewer::Scroll,
        });
    }

    Ok(MangaPageResult { manga: mangas, has_more: true })
}

#[handle_url]
fn handle_url(url: String) -> Result<aidoku::std::json::Object> {
    // Identify whether it's a manga or a chapter URL
    let is_chapter = url.contains("/chapter/") || url.contains("/ch/") || url.contains("/chapter-");
    let obj = if is_chapter {
        json!({
            "type": "chapter",
            "id": url.clone(),
            "url": url,
        })
    } else {
        json!({
            "type": "manga",
            "id": url.clone(),
            "url": url,
        })
    };
    Ok(obj.as_object().unwrap().clone())
}
