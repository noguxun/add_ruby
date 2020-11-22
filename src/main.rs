use chrono::Utc;
use anyhow::Result;
use fastly::http::{HeaderValue, header, Method, StatusCode};
use fastly::{Body, Error, Request, RequestExt, Response, ResponseExt};
use fastly::request::CacheOverride;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use kanji::{is_kanji, is_hiragana};

const BACKEND_NAME: &str = "labs.goo.ne.jp";
const CONTENT_BACKEND: &str = "www.fastly.jp";

#[derive(Serialize, Deserialize)]
struct HiraganaResp {
    converted: String,
    output_type: String,
    request_id: String,
}

struct HtmlPart {
    content: String,
    need_ruby: bool,
}

#[fastly::main]
fn main(mut req: Request<Body>) -> Result<impl ResponseExt, Error> {
    log_fastly::init_simple("my_log", log::LevelFilter::Info);
    fastly::log::set_panic_endpoint("my_log").unwrap();

    // We can filter requests that have unexpected methods.
    const VALID_METHODS: [Method; 3] = [Method::HEAD, Method::GET, Method::POST];
    if !(VALID_METHODS.contains(req.method())) {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::from("This method is not allowed"))?);
    }

    // Make any desired changes to the client request.
    req.headers_mut()
        .insert("Host", HeaderValue::from_static(CONTENT_BACKEND));


    *req.cache_override_mut() = CacheOverride::ttl(60);
    log::info!("time: {},url: {}", Utc::now(), req.uri());
    let resp = req.send(BACKEND_NAME)?;
    if resp.status() == StatusCode::OK {
        let body_string = resp.into_body().into_string();
        let html_parts = analyze_jp(body_string);
        let coverted = generate_html_with_ruby(&html_parts)?;
        return  Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(coverted))?)
    }
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("Welcome to Fastly Compute@Edge!"))?)
 
}

fn analyze_jp(body_string: String) -> Vec<HtmlPart> {
    let chars_num = body_string.as_str().chars().count();
    let html_chars = body_string.as_str().chars().collect::<Vec<char>>();
    let mut i = 0;
    let mut html_parts = Vec::new();
    let mut content = "".to_string();
    while i < chars_num {
        let mut char = html_chars[i];
        if char != '>' {
            content.push(char);
            i += 1;
            continue;
        }
        if char == '>' {
            loop {
                char = html_chars[i];
                let mut next_char;
                if i + 1 < chars_num {
                    next_char = html_chars[i + 1]
                } else {
                    content.push(char);
                    let html_part = HtmlPart {
                        content: content.clone(),
                        need_ruby: false,
                    };
                    html_parts.push(html_part);
                    break;
                }
                if next_char == '<' {
                    if !is_kanji(&char) && !is_hiragana(&char) {
                        content.push(char);
                        i += 1;
                        break;
                    } else {
                        content.push(char);
                        i += 1;
                        let html_part = HtmlPart {
                            content: content,
                            need_ruby: true,
                        };
                        html_parts.push(html_part);
                        content = "".to_string();
                        break;
                    }
                }
                if !is_kanji(&next_char) && !is_hiragana(&next_char) {
                    if !is_kanji(&char) && !is_hiragana(&char) {
                        content.push(char);
                        i += 1;
                    } else {
                        content.push(char);
                        i += 1;
                        let html_part = HtmlPart {
                            content: content,
                            need_ruby: true,
                        };
                        html_parts.push(html_part);
                        content = "".to_string();
                    }
                } else {
                    if !is_kanji(&char) && !is_hiragana(&char) {
                        content.push(char);
                        i += 1;
                        let html_part = HtmlPart {
                            content: content,
                            need_ruby: false,
                        };
                        html_parts.push(html_part);
                        content = "".to_string();
                    } else {
                        content.push(char);
                        i += 1;
                    }
                }
            }
        }
    }
    return html_parts;
}

fn generate_sample_html_parts() -> Vec<HtmlPart> {
    let mut parts = Vec::<HtmlPart>::new();

    parts.push(HtmlPart {
        content: String::from("<html><head><meta charset=\"UTF-8\"></head>"),
        need_ruby: false,
    });

    parts.push(HtmlPart {
        content: String::from("日本語が難しいですよ"),
        need_ruby: true,
    });

    parts.push(HtmlPart {
        content: String::from("</html>"),
        need_ruby: false,
    });

    parts
}

fn generate_html_with_ruby(parts: &Vec<HtmlPart>) -> Result<String> {
    let mut html_page = String::new();
    for part in parts {
        if part.need_ruby {
            let hiragana = get_hiragana(&part.content)?;
            write!(
                &mut html_page,
                "<ruby><rb>{}</rb><rt>{}</rt></ruby>",
                part.content, hiragana
            )?;
        } else {
            write!(&mut html_page, "{}", part.content)?;
        }
    }

    Ok(html_page)
}

fn get_hiragana(j: &str) -> Result<String> {
    let app_id = "57612e6db386dded03ab099ac9afa1276ea7f20f78528b8a5a0717e0e99b69e2";
    let req_body = format!(
        r#"{{
      "app_id": "{}",
      "request_id": "test1",
      "sentence": "{}",
      "output_type": "hiragana"
    }}"#,
        app_id, j
    );

    log::info!("{}", &req_body);

    let req = Request::builder()
        .method(Method::POST)
        .header(header::CONTENT_TYPE, "application/json")
        .uri("https://labs.goo.ne.jp/api/hiragana")
        .body(Body::from(req_body))?;

    let resp = req.send(BACKEND_NAME)?;

    let (_parts, body) = resp.into_parts();
    let body_str = body.into_string();

    log::info!("{}", &body_str);

    let hiragana_resp: HiraganaResp = serde_json::from_str(&body_str)?;

    Ok(hiragana_resp.converted)
}
