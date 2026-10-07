use std::path::Path;

use super::arch::Files;

impl Files {

    pub(super) fn kind ( path: &Path ) -> &'static str {

        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else { return "application/octet-stream"; };

        match extension.to_ascii_lowercase().as_str() {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" | "mjs" => "text/javascript",
            "json" | "map" => "application/json",
            "xml" => "application/xml",
            "txt" | "md" | "log" => "text/plain",
            "csv" => "text/csv",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "avif" => "image/avif",
            "ico" => "image/x-icon",
            "bmp" => "image/bmp",
            "pdf" => "application/pdf",
            "wasm" => "application/wasm",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "ttf" => "font/ttf",
            "otf" => "font/otf",
            "mp4" | "m4v" => "video/mp4",
            "webm" => "video/webm",
            "mp3" => "audio/mpeg",
            "ogg" | "oga" => "audio/ogg",
            "wav" => "audio/wav",
            "zip" => "application/zip",
            "gz" => "application/gzip",
            "br" => "application/brotli",
            "tar" => "application/x-tar",
            "webmanifest" => "application/manifest+json",
            _ => "application/octet-stream",
        }

    }

}
