use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;

use anyhow::{anyhow, bail, Context};
use yaml_rust::yaml::{Yaml, YamlLoader};

mod frontmatter {
    pub const DELIMITER: &str = "---";
    pub const DATE: &str = "date";
    pub const SLUG: &str = "slug";
    pub const TITLE: &str = "title";
    pub const WITHIN_DATE: &str = "withindate";
}

#[derive(Debug, PartialEq, Eq)]
struct Article {
    slug: String,
    title: String,
    body: String,
    date: String,
    within_date: u32,
}

impl Article {
    fn sort_key(&self) -> (&str, u32) {
        (&self.date, self.within_date)
    }
}

fn parse_article_file(path: &Path) -> Result<Article, anyhow::Error> {
    let mut f = BufReader::new(File::open(path)?);
    let mut line = String::new();

    // Find the frontmatter section, delimited by lines containing exactly "---".
    if f.read_line(&mut line)? == 0 {
        bail!("file empty (no frontmatter)");
    }
    if line.trim_end() != frontmatter::DELIMITER {
        bail!("first line of file is not frontmatter delimiter");
    }
    let mut frontmatter = String::new();
    loop {
        line.clear();
        if f.read_line(&mut line)? == 0 {
            bail!("end-of-frontmatter delimiter not found before EOF");
        }
        if line.trim_end() == frontmatter::DELIMITER {
            break;
        }
        frontmatter.push_str(&line);
    }

    // Parse the frontmatter into a valid YAML hash.
    let mut frontmatter =
        YamlLoader::load_from_str(&frontmatter).context("invalid YAML syntax in frontmatter")?;
    if frontmatter.len() != 1 {
        bail!(
            "frontmatter does not contain exactly one YAML document: len = {}",
            frontmatter.len()
        );
    }
    let frontmatter = match &mut frontmatter[0] {
        Yaml::Hash(h) => h,
        _ => bail!("frontmatter is not a YAML hash"),
    };

    // Extract keys of interest from the YAML frontmatter.
    let mut get_key = |k: &'static str| {
        frontmatter
            .get_mut(&Yaml::String(String::from(k)))
            .map(|x| std::mem::replace(x, Yaml::Null))
            .ok_or_else(|| anyhow!("missing frontmatter key: {}", k))
    };
    let mut get_string_key = |k| {
        get_key(k)?
            .into_string()
            .ok_or_else(|| anyhow!("invalid frontmatter value for {}", k))
    };
    let slug = get_string_key(frontmatter::SLUG)?;
    let title = get_string_key(frontmatter::TITLE)?;
    let date = get_string_key(frontmatter::DATE)?;
    let within_date = get_key(frontmatter::WITHIN_DATE)
        .unwrap_or(Yaml::Integer(0))
        .into_i64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| anyhow!("invalid frontmatter value for {}", frontmatter::WITHIN_DATE))?;

    // Skip blank lines between frontmatter and article. (Probably not strictly necessary given
    // that we'll parse the file as Markdown.)
    let mut body = String::new();
    loop {
        f.read_line(&mut body)?;
        if !body.trim().is_empty() {
            break;
        }
        body.clear();
    }
    f.read_to_string(&mut body)?;

    Ok(Article {
        slug,
        title,
        date,
        within_date,
        body,
    })
}

fn collect_articles(dir: &Path) -> Result<Vec<Article>, anyhow::Error> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("opening directory {:?}", dir))? {
        let entry = entry?;
        if !entry
            .file_type()
            .with_context(|| format!("checking file type for {:?}", entry.file_name()))?
            .is_file()
        {
            continue;
        }
        result.push(
            parse_article_file(&entry.path())
                .with_context(|| format!("reading file {:?}", entry.file_name()))?,
        );
    }
    result.sort_by(|a1, a2| a1.sort_key().cmp(&a2.sort_key()));
    Ok(result)
}

const STYLESHEET: &str = "\
html {
    box-sizing: border-box;
}
body {
    max-width: 800px;
    margin: 0 auto;
    background-color: #fbfbe9;
    font-family: Georgia, serif;
    font-size: 20px;
    line-height: 1.7;
}
h1 {
    margin-top: 2em;
    line-height: 1.4;
}
.toc {
    list-style: '\\200B';
    line-height: 1.4;
}
.toc .date {
    font-size: smaller;
    padding-right: 1em;
}
article img {
    max-width: 100%;
}
";

fn write_index_html<W: Write>(mut writer: W, articles: &[Article]) -> Result<(), anyhow::Error> {
    write!(
        &mut writer,
        "\
        <!DOCTYPE html>\n\
        <link rel=\"stylesheet\" href=\"styles.css\">\n\
        <title>Index</title>\n\
        <h1>Index</h1>\n\
        <ul class=\"toc\">\n\
        "
    )?;
    for article in articles {
        // deliberately not worrying about escaping; trusted sources
        writeln!(
            &mut writer,
            "<li><span class=\"date\">{date}</span><a href=\"{slug}.html\">{title}</a></li>",
            slug = &article.slug,
            title = &article.title,
            date = article.date,
        )?;
    }
    writeln!(&mut writer, "</ul>")?;
    Ok(())
}

/// Flushes and syncs all writes before closing the writer.
fn safe_close(writer: BufWriter<File>) -> Result<(), anyhow::Error> {
    writer
        .into_inner()
        .context("flushing file")?
        .sync_all()
        .context("closing file")?;
    Ok(())
}

fn render_site(articles: &[Article], output_dir: &Path) -> Result<(), anyhow::Error> {
    {
        // index.html
        let outfile = output_dir.join("index.html");
        let mut writer = BufWriter::new(File::create(&outfile).context("creating index.html")?);
        write_index_html(&mut writer, articles).context("writing to index.html")?;
        safe_close(writer).context("index.html")?;
    }

    {
        // styles.css
        let outfile = output_dir.join("styles.css");
        std::fs::write(&outfile, STYLESHEET).context("styles.css")?;
    }

    for article in articles {
        let parser = pulldown_cmark::Parser::new(&article.body);
        let outfile = output_dir.join(format!("{}.html", &article.slug));
        let mut writer = BufWriter::new(
            File::create(&outfile).with_context(|| format!("creating file {:?}", outfile))?,
        );
        write!(
            &mut writer,
            "\
            <!DOCTYPE html>\n\
            <link rel=\"stylesheet\" href=\"styles.css\">\n\
            <title>{title}</title>\n\
            <article>\n\
            ",
            title = &article.title,
        )
        .with_context(|| format!("writing to {:?}", outfile))?;
        pulldown_cmark::html::write_html(&mut writer, parser)
            .with_context(|| format!("writing to {:?}", outfile))?;
        safe_close(writer).with_context(|| format!("{:?}", outfile))?;
    }
    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: mksite <articles-dir> <output-dir>");
        std::process::exit(1);
    }
    let articles_dir = Path::new(args[0].as_os_str());
    let output_dir = Path::new(args[1].as_os_str());
    let articles = collect_articles(articles_dir).context("collecting articles")?;
    render_site(&articles, output_dir).context("rendering site")?;
    Ok(())
}
