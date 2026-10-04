//! Disposable static rendering for human review, separate from guide generation.
use pulldown_cmark::{Event, Options, Parser, Tag, html};
use std::{env, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: preview EXPORT_DIRECTORY NEW_PREVIEW_DIRECTORY".into());
    }
    let destination = Path::new(&args[1]);
    preview(Path::new(&args[0]), destination)?;
    println!("Preview: {}", destination.display());
    Ok(())
}

fn preview(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = source.canonicalize()?;
    let destination = executable_docs::lifecycle::absolute(destination)?;
    executable_docs::lifecycle::ensure_separate_paths(&source, &destination)?;
    if !source.is_dir() {
        return Err("Preview source must be an exported directory".into());
    }
    if destination.exists() {
        return Err("Preview destination must be new; preserve earlier review evidence".into());
    }
    fs::create_dir_all(&destination)?;
    render_tree(&source, &destination)?;
    Ok(())
}

fn preview_link(destination: &str) -> String {
    if destination.contains(':') || destination.starts_with("//") {
        return destination.to_owned();
    }
    let (path, fragment) = destination.split_once('#').unwrap_or((destination, ""));
    match path.strip_suffix(".md") {
        Some(path) if fragment.is_empty() => format!("{path}.html"),
        Some(path) => format!("{path}.html#{fragment}"),
        None => destination.to_owned(),
    }
}

fn render_tree(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!("Preview refuses symlink: {}", entry.path().display()).into());
        }
        let output = destination.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir(&output)?;
            render_tree(&entry.path(), &output)?;
        } else if entry.path().extension().is_some_and(|ext| ext == "md") {
            let source = fs::read_to_string(entry.path())?;
            let parser =
                Parser::new_ext(&source, Options::ENABLE_TABLES).map(|event| match event {
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }) => Event::Start(Tag::Link {
                        link_type,
                        dest_url: preview_link(&dest_url).into(),
                        title,
                        id,
                    }),
                    other => other,
                });
            let mut body = String::new();
            html::push_html(&mut body, parser);
            fs::write(
                output.with_extension("html"),
                format!(
                    "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Executable documentation review</title><style>{STYLE}</style><main>{body}</main></html>"
                ),
            )?;
        } else if entry.file_name() != ".ownership.json" {
            fs::copy(entry.path(), output)?;
        }
    }
    Ok(())
}

const STYLE: &str = r#"
html { color-scheme: light; background: #f5f6f8; color: #17212b; }
body { margin: 0; font: 17px/1.65 -apple-system, BlinkMacSystemFont, sans-serif; }
main { max-width: 800px; margin: 42px auto; padding: 36px 48px; background: white; border: 1px solid #dde2e7; border-radius: 12px; }
h1, h2, h3 { line-height: 1.2; color: #0b243c; } h1 { font-size: 34px; } h2 { margin-top: 38px; }
a { color: #005d9f; } code { font: 0.88em/1.5 ui-monospace, monospace; background: #f1f3f6; padding: 2px 5px; border-radius: 3px; }
pre { overflow: auto; padding: 18px; border: 1px solid #dce3e9; border-radius: 6px; background: #f7f9fb; } pre code { padding: 0; background: transparent; }
blockquote { margin: 24px 0; border-left: 4px solid #ae7c19; background: #fff8e6; padding: 8px 20px; }
img { max-width: 100%; } table { border-collapse: collapse; width: 100%; } th, td { text-align: left; border: 1px solid #dce3e9; padding: 8px; }
@media(max-width: 700px) { main { margin: 0; padding: 22px; border: 0; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_rejects_overlapping_trees_before_creating_output() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let source = root.join("export");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("guide.md"), "Retain the source.").unwrap();
        for destination in [&source, &source.join("preview"), &root] {
            assert!(preview(&source, destination).is_err());
        }
        assert_eq!(fs::read_dir(&source).unwrap().count(), 1);
        assert_eq!(
            fs::read_to_string(source.join("guide.md")).unwrap(),
            "Retain the source."
        );
    }

    #[test]
    fn preview_rewrites_local_pages_and_preserves_external_destinations() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let source = root.join("export");
        let destination = root.join("preview");
        fs::create_dir_all(source.join("nested")).unwrap();
        let markdown = "[Local](nested/other.md#section)\n\
            [Remote](https://example.com/other.md#section)\n\
            [Email](mailto:editor@example.md)\n\
            [Network](//example.com/other.md)\n";
        fs::write(source.join("guide.md"), markdown).unwrap();
        fs::write(source.join("nested/other.md"), "# Other").unwrap();
        fs::write(source.join(".fixture.txt"), "A documented hidden file").unwrap();
        fs::write(source.join(".ownership.json"), "store metadata").unwrap();
        preview(&source, &destination).unwrap();
        let html = fs::read_to_string(destination.join("guide.html")).unwrap();
        for target in [
            "nested/other.html#section",
            "https://example.com/other.md#section",
            "mailto:editor@example.md",
            "//example.com/other.md",
        ] {
            assert!(html.contains(&format!("href=\"{target}\"")), "{html}");
        }
        assert!(destination.join("nested/other.html").is_file());
        assert_eq!(
            fs::read_to_string(destination.join(".fixture.txt")).unwrap(),
            "A documented hidden file"
        );
        assert!(!destination.join(".ownership.json").exists());
        assert_eq!(
            fs::read_to_string(source.join("guide.md")).unwrap(),
            markdown
        );
    }
}
