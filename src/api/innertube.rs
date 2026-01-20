use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Url;
use serde_json::{json, Value};

const BASE_URL: &str = "https://youtubei.googleapis.com/youtubei/v1/";
const MUSIC_SEARCH_PARAMS: &str = "EgWKAQIIAWoKEAMQBBAJEAoQBQ%3D%3D";
const LRC_SEARCH_URL: &str = "https://lrclib.net/api/search";

const USER_AGENT_WEB: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/74.0.3729.157 Safari/537.36";
const USER_AGENT_ANDROID: &str = "Mozilla/5.0 (Linux; Android 6.0; Nexus 5 Build/MRA58N) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/65.0.3325.181 Mobile Safari/537.36";

const REFERER_WEB_MUSIC: &str = "https://music.youtube.com/";

struct ClientProfile {
    client_id: &'static str,
    client_name: &'static str,
    client_version: &'static str,
    api_key: &'static str,
    user_agent: &'static str,
    referer: Option<&'static str>,
}

const WEB_REMIX: ClientProfile = ClientProfile {
    client_id: "67",
    client_name: "WEB_REMIX",
    client_version: "1.20230724.00.00",
    api_key: "AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30",
    user_agent: USER_AGENT_WEB,
    referer: Some(REFERER_WEB_MUSIC),
};

const ANDROID: ClientProfile = ClientProfile {
    client_id: "3",
    client_name: "ANDROID",
    client_version: "19.17.34",
    api_key: "AIzaSyA8eiZmM1FaDVjRy-df2KTyQ_vz_yYM39w",
    user_agent: USER_AGENT_ANDROID,
    referer: None,
};

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub duration: String,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchPage {
    pub results: Vec<SearchResult>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub url: String,
    pub title: String,
    pub artist: String,
    pub thumbnail_url: Option<String>,
    pub lyrics: Option<Vec<LyricLine>>,
}

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub timestamp: f64,
    pub text: String,
}

pub struct InnertubeClient {
    client: Client,
    visitor_id: Option<String>,
}

impl InnertubeClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .build()
            .expect("innertube http client");
        Self {
            client,
            visitor_id: None,
        }
    }

    pub fn search_music(&mut self, query: &str) -> Result<Vec<SearchResult>, String> {
        let page = self.search_music_page(query, None)?;
        Ok(page.results)
    }

    pub fn search_music_page(
        &mut self,
        query: &str,
        continuation: Option<&str>,
    ) -> Result<SearchPage, String> {
        let mut body = json!({
            "context": {
                "client": {
                    "clientName": WEB_REMIX.client_name,
                    "clientVersion": WEB_REMIX.client_version,
                }
            }
        });

        if let Some(token) = continuation {
            body["continuation"] = json!(token);
        } else {
            body["query"] = json!(query);
            body["params"] = json!(MUSIC_SEARCH_PARAMS);
        }

        let value = self.post_json(&WEB_REMIX, "search", body)?;
        parse_music_search_results_with_continuation(&value)
    }

    pub fn stream_info(&mut self, video_id: &str) -> Result<StreamInfo, String> {
        let body = json!({
            "context": {
                "client": {
                    "clientName": ANDROID.client_name,
                    "clientVersion": ANDROID.client_version,
                }
            },
            "videoId": video_id,
        });

        let value = self.post_json(&ANDROID, "player", body)?;
        let status = value
            .pointer("/playabilityStatus/status")
            .and_then(|value| value.as_str())
            .unwrap_or("ERROR");
        if status != "OK" {
            let reason = value
                .pointer("/playabilityStatus/reason")
                .and_then(|value| value.as_str())
                .unwrap_or("Not available");
            return Err(reason.to_string());
        }

        let mut best_url = None;
        let mut best_bitrate = 0;
        if let Some(formats) = value.pointer("/streamingData/adaptiveFormats") {
            if let Some(items) = formats.as_array() {
                for item in items {
                    let mime = item.get("mimeType").and_then(|value| value.as_str()).unwrap_or("");
                    let url = item.get("url").and_then(|value| value.as_str());
                    if !mime.contains("audio") || url.is_none() {
                        continue;
                    }
                    let bitrate = item.get("bitrate").and_then(|value| value.as_i64()).unwrap_or(0);
                    if bitrate > best_bitrate {
                        best_bitrate = bitrate;
                        best_url = url.map(|value| value.to_string());
                    }
                }
            }
        }

        let url = best_url.ok_or_else(|| "No audio stream found".to_string())?;
        let title = value
            .pointer("/videoDetails/title")
            .and_then(|value| value.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let artist = value
            .pointer("/videoDetails/author")
            .and_then(|value| value.as_str())
            .unwrap_or("Unknown")
            .replace(" - Topic", "");
        let video_thumbnail_url = value
            .pointer("/videoDetails/thumbnail/thumbnails")
            .and_then(|value| value.as_array())
            .and_then(|items| items.last())
            .and_then(|value| value.get("url"))
            .and_then(|value| value.as_str())
            .and_then(normalize_thumbnail_url);
        let clean_title = clean_track_title(&title);
        let clean_artist = clean_track_artist(&artist);
        let music_cover_url = self.fetch_music_cover(&clean_title, &clean_artist);
        let lyrics = self.fetch_synced_lyrics(&clean_title, &clean_artist);

        Ok(StreamInfo {
            url,
            title,
            artist,
            thumbnail_url: music_cover_url.or(video_thumbnail_url),
            lyrics,
        })
    }

    fn post_json(&mut self, profile: &ClientProfile, endpoint: &str, body: Value) -> Result<Value, String> {
        let url = format!("{BASE_URL}{endpoint}?key={}&alt=json", profile.api_key);
        let headers = build_headers(profile, self.visitor_id.as_deref());
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .map_err(|error| format!("Network error: {error}"))?;

        let value: Value = response
            .json()
            .map_err(|error| format!("Invalid response: {error}"))?;

        if let Some(visitor) = value
            .pointer("/responseContext/visitorData")
            .and_then(|data| data.as_str())
        {
            self.visitor_id = Some(visitor.to_string());
        }

        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("Innertube error");
            return Err(message.to_string());
        }

        Ok(value)
    }
}

fn build_headers(profile: &ClientProfile, visitor_id: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "X-Goog-Api-Format-Version",
        HeaderValue::from_static("1"),
    );
    headers.insert(
        "X-YouTube-Client-Name",
        HeaderValue::from_static(profile.client_id),
    );
    headers.insert(
        "X-YouTube-Client-Version",
        HeaderValue::from_static(profile.client_version),
    );
    headers.insert(
        "User-Agent",
        HeaderValue::from_str(profile.user_agent).expect("user agent header"),
    );
    if let Some(referer) = profile.referer {
        headers.insert("Referer", HeaderValue::from_static(referer));
    }

    if let Some(visitor) = visitor_id {
        if let Ok(value) = HeaderValue::from_str(visitor) {
            headers.insert("X-Goog-Visitor-Id", value);
        }
    }

    headers
}

fn clean_track_title(title: &str) -> String {
    title
        .replace("(Official Music Video)", "")
        .replace("(Official Video)", "")
        .replace("(Official Lyric Video)", "")
        .replace("(Lyric Video)", "")
        .trim()
        .to_string()
}

fn clean_track_artist(artist: &str) -> String {
    artist.replace(" - Topic", "").trim().to_string()
}

fn upgrade_music_thumbnail(url: &str) -> String {
    url.replace("w120-h120", "w500-h500")
        .replace("w60-h60", "w500-h500")
        .replace("w120-h120-rj", "w500-h500")
}

fn extract_first_music_thumbnail(value: &Value) -> Option<String> {
    let contents = value
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|value| value.as_array())?;

    for section in contents {
        let Some(items) = section
            .pointer("/musicShelfRenderer/contents")
            .and_then(|value| value.as_array())
        else {
            continue;
        };

        for item in items {
            let renderer = item.get("musicResponsiveListItemRenderer")?;
            let url = renderer
                .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
                .and_then(|value| value.as_array())
                .and_then(|items| items.last())
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str())
                .and_then(normalize_thumbnail_url)?;
            return Some(upgrade_music_thumbnail(&url));
        }
    }

    None
}

impl InnertubeClient {
    fn fetch_music_cover(&mut self, title: &str, artist: &str) -> Option<String> {
        let query = format!("{title} {artist}").trim().to_string();
        if query.is_empty() {
            return None;
        }

        let body = json!({
            "context": {
                "client": {
                    "clientName": WEB_REMIX.client_name,
                    "clientVersion": WEB_REMIX.client_version,
                }
            },
            "query": query,
            "params": MUSIC_SEARCH_PARAMS,
        });

        let value = self.post_json(&WEB_REMIX, "search", body).ok()?;
        extract_first_music_thumbnail(&value)
    }

    fn fetch_synced_lyrics(&mut self, title: &str, artist: &str) -> Option<Vec<LyricLine>> {
        if title.trim().is_empty() {
            return None;
        }

        let url = Url::parse_with_params(
            LRC_SEARCH_URL,
            &[("track_name", title), ("artist_name", artist)],
        )
        .ok()?;
        let response = self
            .client
            .get(url)
            .header("User-Agent", "Musika/0.1.0")
            .send()
            .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let value: Value = response.json().ok()?;
        let array = value.as_array()?;
        let first = array.first()?;
        let synced = first.get("syncedLyrics").and_then(Value::as_str)?;
        let lines = parse_synced_lyrics(synced);
        if lines.is_empty() {
            None
        } else {
            Some(lines)
        }
    }
}

fn parse_music_search_results_with_continuation(value: &Value) -> Result<SearchPage, String> {
    let (items, continuation) = collect_music_search_items(value)?;
    let results = parse_music_search_items(&items);

    Ok(SearchPage {
        results,
        continuation,
    })
}

fn collect_music_search_items(value: &Value) -> Result<(Vec<Value>, Option<String>), String> {
    let mut items = Vec::new();
    let mut continuation = None;

    if let Some(sections) = value
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|value| value.as_array())
    {
        for section in sections {
            let Some(music_shelf) = section.get("musicShelfRenderer") else {
                continue;
            };
            if let Some(contents) = music_shelf.get("contents").and_then(|value| value.as_array()) {
                for item in contents {
                    items.push(item.clone());
                }
            }
            if continuation.is_none() {
                continuation = music_shelf
                    .pointer("/continuations/0/nextContinuationData/continuation")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
            }
        }

        if !items.is_empty() || continuation.is_some() {
            return Ok((items, continuation));
        }
    }

    if let Some(continuation_contents) = value.pointer("/continuationContents/musicShelfContinuation") {
        if let Some(contents) = continuation_contents
            .get("contents")
            .and_then(|value| value.as_array())
        {
            for item in contents {
                items.push(item.clone());
            }
        }

        continuation = continuation_contents
            .pointer("/continuations/0/nextContinuationData/continuation")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());

        if !items.is_empty() || continuation.is_some() {
            return Ok((items, continuation));
        }
    }

    if let Some(commands) = value
        .pointer("/onResponseReceivedCommands")
        .and_then(|value| value.as_array())
    {
        for command in commands {
            let Some(continuation_items) = command
                .pointer("/appendContinuationItemsAction/continuationItems")
                .and_then(|value| value.as_array())
            else {
                continue;
            };

            for item in continuation_items {
                if item.get("musicResponsiveListItemRenderer").is_some() {
                    items.push(item.clone());
                }
                if continuation.is_none() {
                    continuation = item
                        .pointer(
                            "/continuationItemRenderer/continuationEndpoint/continuationCommand/token",
                        )
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string());
                }
            }
        }
    }

    if !items.is_empty() || continuation.is_some() {
        return Ok((items, continuation));
    }

    Err("No search results".to_string())
}

fn parse_music_search_items(items: &[Value]) -> Vec<SearchResult> {
    let mut results = Vec::new();
    for item in items {
        let renderer = match item.get("musicResponsiveListItemRenderer") {
            Some(value) => value,
            None => continue,
        };

        let video_id = renderer
            .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if video_id.is_empty() {
            continue;
        }

        let empty_columns: &[Value] = &[];
        let flex_columns = renderer
            .get("flexColumns")
            .and_then(|value| value.as_array())
            .map_or(empty_columns, |value| value.as_slice());
        let title = flex_columns
            .first()
            .and_then(|value| value.pointer("/musicResponsiveListItemFlexColumnRenderer/text"))
            .and_then(extract_text)
            .unwrap_or_else(|| "Unknown".to_string());

        let mut artist = String::new();
        let mut album = String::new();
        if let Some(secondary) = flex_columns
            .get(1)
            .and_then(|value| value.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
            .and_then(|value| value.as_array())
        {
            for run in secondary {
                let page_type = run
                    .pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let text = run.get("text").and_then(|value| value.as_str()).unwrap_or("");
                if page_type == "MUSIC_PAGE_TYPE_ARTIST" {
                    artist = text.to_string();
                } else if page_type == "MUSIC_PAGE_TYPE_ALBUM" {
                    album = text.to_string();
                }
            }

            if artist.is_empty() {
                artist = secondary
                    .first()
                    .and_then(|value| value.get("text"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
            }
        }

        let duration = extract_duration(renderer).unwrap_or_else(|| "--:--".to_string());
        let thumbnail_url = renderer
            .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
            .and_then(|value| value.as_array())
            .and_then(|items| items.last())
            .and_then(|value| value.get("url"))
            .and_then(|value| value.as_str())
            .and_then(normalize_thumbnail_url);

        let artist = if artist.is_empty() { album } else { artist };
        results.push(SearchResult {
            video_id: video_id.to_string(),
            title,
            artist: if artist.is_empty() { "Unknown".to_string() } else { artist },
            duration,
            thumbnail_url,
        });
    }

    results
}

fn parse_synced_lyrics(lyrics: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();

    for raw in lyrics.lines() {
        let mut timestamps = Vec::new();
        let mut index = 0;

        while let Some(start) = raw[index..].find('[') {
            let start = index + start;
            let Some(end_offset) = raw[start + 1..].find(']') else {
                break;
            };
            let end = start + 1 + end_offset;
            let time_str = &raw[start + 1..end];
            if let Some(timestamp) = parse_lyric_timestamp(time_str) {
                timestamps.push(timestamp);
            }
            index = end + 1;
        }

        let text = raw[index..].trim();
        if text.is_empty() {
            continue;
        }

        for timestamp in timestamps {
            lines.push(LyricLine {
                timestamp,
                text: text.to_string(),
            });
        }
    }

    lines.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

fn parse_lyric_timestamp(value: &str) -> Option<f64> {
    let mut parts = value.split(':');
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds_part = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let (secs, frac) = if let Some((secs, frac)) = seconds_part.split_once('.') {
        (secs, Some(frac))
    } else {
        (seconds_part, None)
    };

    let secs: f64 = secs.parse().ok()?;
    let fractional = if let Some(frac) = frac {
        let value: f64 = frac.parse().ok()?;
        let divisor = 10f64.powi(frac.len() as i32);
        value / divisor
    } else {
        0.0
    };

    Some(minutes * 60.0 + secs + fractional)
}

fn extract_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("simpleText").and_then(|text| text.as_str()) {
        return Some(text.to_string());
    }

    if let Some(runs) = value.get("runs").and_then(|runs| runs.as_array()) {
        let mut output = String::new();
        for run in runs {
            if let Some(text) = run.get("text").and_then(|text| text.as_str()) {
                output.push_str(text);
            }
        }
        if !output.is_empty() {
            return Some(output);
        }
    }

    None
}

fn extract_duration(renderer: &Value) -> Option<String> {
    if let Some(value) = renderer
        .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text")
        .and_then(extract_text)
    {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    if let Some(value) = renderer.get("lengthText").and_then(extract_text) {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    None
}

fn normalize_thumbnail_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(stripped) = trimmed.strip_prefix("//") {
        return Some(format!("https://{stripped}"));
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    None
}
