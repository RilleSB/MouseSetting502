//! Read-only release feed, loaded on demand without blocking the UI or hashing the EXE.
use super::{
    bridge::{MainWindow, ReleaseItem},
    updater,
};
use serde::Deserialize;
use slint::{ComponentHandle, ModelRc, VecModel};

const FEED_URL: &str = "https://api.github.com/repos/RilleSB/MouseSetting502/releases?per_page=10";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

pub(super) fn setup(app: &MainWindow) {
    let weak = app.as_weak();
    app.on_load_community(move |force| {
        let Some(app)=weak.upgrade() else{return;};
        if app.get_community_loading() || (app.get_community_loaded() && !force) {return;}
        app.set_community_loading(true);
        app.set_community_error("".into());
        let weak=weak.clone();
        std::thread::spawn(move || {
            let result=updater::fetch_github_json(FEED_URL).and_then(|json|parse_feed(&json));
            let _=slint::invoke_from_event_loop(move || {
                if let Some(app)=weak.upgrade() {
                    app.set_community_loading(false);
                    match result {
                        Ok(items)=> {
                            app.set_community_items(ModelRc::new(VecModel::from(items)));
                            app.set_community_loaded(true);
                        }
                        Err(error)=>app.set_community_error(format!("Не удалось загрузить новости. Проверьте подключение и нажмите «Обновить». {error}").into()),
                    }
                }
            });
        });
    });
    app.on_open_community_release(move |url| {
        if valid_release_url(url.as_str()) {
            updater::open_releases(Some(url.as_str()));
        }
    });
}

fn valid_release_url(url: &str) -> bool {
    url.starts_with(&format!("{}/releases/tag/", updater::REPO_URL))
}

fn parse_feed(json: &[u8]) -> Result<Vec<ReleaseItem>, String> {
    let mut releases: Vec<Release> = serde_json::from_slice(json).map_err(|_| {
        "GitHub вернул некорректный ответ или ограничил частоту запросов.".to_string()
    })?;
    releases.retain(|r| {
        !r.draft
            && !r.tag_name.trim().is_empty()
            && r.published_at.is_some()
            && valid_release_url(&r.html_url)
    });
    releases.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    Ok(releases
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, r)| {
            let title = r
                .name
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| r.tag_name.clone());
            let date = r
                .published_at
                .unwrap_or_default()
                .chars()
                .take(10)
                .collect::<String>();
            ReleaseItem {
                tag: r.tag_name.into(),
                title: title.into(),
                date: date.into(),
                summary: compact_notes(r.body.as_deref().unwrap_or("")).into(),
                url: r.html_url.into(),
                prerelease: r.prerelease,
                latest: index == 0,
            }
        })
        .collect())
}

/// A bounded excerpt of the author's notes; no HTML, remote images or executable links.
fn compact_notes(body: &str) -> String {
    let mut lines = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("```") || line.starts_with("![") {
            continue;
        }
        let line = line.trim_start_matches('#').trim();
        let line = if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            format!("• {item}")
        } else {
            line.to_string()
        };
        lines.push(line.replace("**", "").replace('`', ""));
        if lines.len() == 8 {
            break;
        }
    }
    if lines.is_empty() {
        return "Автор не добавил описание этого обновления. Подробности доступны на странице релиза.".into();
    }
    let text = lines.join("\n");
    if text.chars().count() > 1200 {
        format!("{}…", text.chars().take(1200).collect::<String>())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feed_filters_drafts_and_foreign_links_and_orders_by_publication() {
        let make = |tag: &str, date: &str, draft: bool| serde_json::json!({"tag_name":tag,"html_url":format!("{}/releases/tag/{tag}",updater::REPO_URL),"published_at":date,"draft":draft,"name":null,"body":"## Исправления\n- **DPI** и `макросы`"});
        let mut foreign = make("foreign", "2026-11-03T00:00:00Z", false);
        foreign["html_url"] = serde_json::json!("https://example.com/");
        let json = serde_json::to_vec(&vec![
            make("old", "2026-10-30T00:00:00Z", false),
            make("draft", "2026-11-04T00:00:00Z", true),
            foreign,
            make("new", "2026-11-02T00:00:00Z", false),
        ])
        .unwrap();
        let items = parse_feed(&json).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].tag, "new");
        assert!(items[0].latest);
        assert!(!items[1].latest);
        assert_eq!(items[0].summary, "Исправления\n• DPI и макросы");
    }
    #[test]
    fn empty_feed_and_missing_notes_are_distinct_from_api_errors() {
        assert!(parse_feed(b"[]").unwrap().is_empty());
        assert!(parse_feed(br#"{"message":"API rate limit exceeded"}"#).is_err());
        assert!(compact_notes("").contains("не добавил"));
        assert!(!valid_release_url("javascript:alert(1)"));
    }
    #[test]
    fn notes_are_bounded_and_keep_unicode_intact() {
        let notes = compact_notes(&"я".repeat(2000));
        assert_eq!(notes.chars().count(), 1201);
        assert!(notes.ends_with('…'));
        assert_eq!(
            compact_notes("- a\n- b\n- c\n- d\n- e\n- f\n- g\n- h\n- i")
                .lines()
                .count(),
            8
        );
    }
}
