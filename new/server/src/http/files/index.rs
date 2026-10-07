use std::path::Path;
use std::time::UNIX_EPOCH;

use http::Method;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderValue};

use crate::http::body::Body;
use crate::http::response::{Res, Response};
use super::arch::Files;

const ENTRIES_MAX: usize = 10_000;

struct Row {
    name     : String,
    dir      : bool,
    size     : u64,
    modified : Option<u64>,
}

impl Files {

    pub(super) async fn listing ( &self, dir: &Path, path: &str, method: &Method ) -> Res {

        let mut reader = match tokio::fs::read_dir(dir).await { Ok(reader) => reader, Err(error) => return Self::failure(dir, &error) };
        let mut rows: Vec<Row> = Vec::new();

        while let Ok(Some(entry)) = reader.next_entry().await {

            if rows.len() >= ENTRIES_MAX { break; }

            let Ok(meta) = entry.metadata().await else { continue; };
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.starts_with('.') { continue; }

            rows.push(Row { name, dir: meta.is_dir(), size: meta.len(), modified: meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|since| since.as_secs()) });

        }

        rows.sort_by(|left, right| right.dir.cmp(&left.dir).then_with(|| left.name.cmp(&right.name)));

        let title = Self::escape(path);
        let mut html = String::with_capacity(256 + rows.len() * 128);

        html.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Index of ");
        html.push_str(&title);
        html.push_str("</title><style>body{font-family:monospace}td{padding:0 1em 0 0}</style></head><body><h1>Index of ");
        html.push_str(&title);
        html.push_str("</h1><table>");

        if path != "/" { html.push_str("<tr><td><a href=\"../\">../</a></td><td>-</td><td>-</td></tr>"); }

        for row in &rows {

            let shown = if row.dir { format!("{}/", row.name) } else { row.name.clone() };

            html.push_str("<tr><td><a href=\"");
            html.push_str(&Self::encode(&shown));
            html.push_str("\">");
            html.push_str(&Self::escape(&shown));
            html.push_str("</a></td><td>");
            html.push_str(&row.modified.map_or_else(|| "-".to_string(), |seconds| httpdate::fmt_http_date(UNIX_EPOCH + std::time::Duration::from_secs(seconds))));
            html.push_str("</td><td>");
            html.push_str(&if row.dir { "-".to_string() } else { row.size.to_string() });
            html.push_str("</td></tr>");

        }

        html.push_str("</table></body></html>");

        let mut response = Response::status(200);

        response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(html.len()));

        if method != Method::HEAD { *response.body_mut() = Body::bytes(html); }

        response

    }

    fn escape ( text: &str ) -> String {

        let mut out = String::with_capacity(text.len());

        for ch in text.chars() {

            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                other => out.push(other),
            }

        }

        out

    }

    fn encode ( text: &str ) -> String {

        let mut out = String::with_capacity(text.len());

        for byte in text.bytes() {

            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(byte as char),
                other => out.push_str(&format!("%{other:02X}")),
            }

        }

        out

    }

}
