use std::{env, fs, path::{Path, PathBuf}};

fn visit ( root: &Path, directory: &Path, files: &mut Vec<PathBuf> ) {
    let mut entries: Vec<_> = fs::read_dir(directory).expect("Read panel export").map(|entry| entry.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() { visit(root, &path, files); }
        else if path.extension().is_none_or(|extension| extension != "map") {
            assert!(path.canonicalize().unwrap().starts_with(root), "Panel assets must stay inside export");
            files.push(path);
        }
    }
}

fn main () {
    let schema_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../model/src/aegisx_model/features.json");
    println!("cargo:rerun-if-changed={}", schema_path.display());
    let schema: serde_json::Value = serde_json::from_slice(&fs::read(schema_path).expect("Read feature schema")).expect("Valid feature schema");
    let count = schema["features"].as_array().expect("Features").len();
    let version = schema["version"].as_u64().expect("Feature version");
    let buckets = schema["lexical"]["buckets_per_width"].as_u64().expect("Lexical buckets") as usize;
    assert!(buckets.is_power_of_two() && count == 40 + 2*buckets);
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("feature_schema.rs"),
        format!("pub const FEATURE_COUNT: usize = {count};\npub const FEATURE_VERSION: u32 = {version};\npub const LEXICAL_BUCKETS: usize = {buckets};\npub const CONTENT_COUNT: usize = 16 + 2*LEXICAL_BUCKETS;\npub type Vector = [f32; FEATURE_COUNT];\n")).unwrap();
    let lifecycle_path=PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../model/src/aegisx_model/lifecycle.json");
    println!("cargo:rerun-if-changed={}",lifecycle_path.display());
    let lifecycle:serde_json::Value=serde_json::from_slice(&fs::read(lifecycle_path).expect("Read lifecycle schema")).expect("Valid lifecycle schema");
    let mut constants=format!("pub const INPUT_SCHEMA: &str = {:?};\n",lifecycle["name"].as_str().unwrap());
    for (key,name) in [("text_bytes","TEXT_BYTES"),("event_count","EVENT_COUNT"),("event_bytes","EVENT_BYTES"),("event_values","EVENT_VALUES")] {
        constants.push_str(&format!("pub const {name}: usize = {};\n",lifecycle[key].as_u64().unwrap()));
    }
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("lifecycle_schema.rs"),constants).unwrap();
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../panel/out");
    println!("cargo:rerun-if-changed={}", root.display());
    assert!(root.join("index.html").is_file(), "Build the embedded panel first: cd ../panel && npm ci && npm run build");
    let root = root.canonicalize().unwrap();
    let mut files = Vec::new();
    visit(&root, &root, &mut files);
    let mut generated = String::from("pub static ASSETS: &[(&str, &[u8], &str)] = &[\n");
    for path in files {
        let name = format!("/{}", path.strip_prefix(&root).unwrap().to_string_lossy());
        let mime = match path.extension().and_then(|extension| extension.to_str()).unwrap_or("") {
            "html" => "text/html; charset=utf-8", "js" => "text/javascript; charset=utf-8",
            "css" => "text/css; charset=utf-8", "json" => "application/json",
            "svg" => "image/svg+xml", "woff2" => "font/woff2", "ico" => "image/x-icon",
            _ => "text/plain; charset=utf-8",
        };
        generated.push_str(&format!("({name:?}, include_bytes!({:?}), {mime:?}),\n", path.to_str().unwrap()));
    }
    generated.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("panel_assets.rs"), generated).unwrap();
}
