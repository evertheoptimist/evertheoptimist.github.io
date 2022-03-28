use std::fs::File;
use std::io::{BufRead, BufReader, Read};
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SortOrdinal {
    date: String,
    within_date: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct Article {
    slug: String,
    title: String,
    body: String,
    sort_ordinal: SortOrdinal,
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
    let slug = get_key(frontmatter::SLUG)?
        .into_string()
        .ok_or_else(|| anyhow!("invalid frontmatter value for {}", frontmatter::SLUG))?;
    let title = get_key(frontmatter::TITLE)?
        .into_string()
        .ok_or_else(|| anyhow!("invalid frontmatter value for {}", frontmatter::TITLE))?;
    let sort_ordinal = SortOrdinal {
        date: get_key(frontmatter::DATE)?
            .into_string()
            .ok_or_else(|| anyhow!("invalid frontmatter value for {}", frontmatter::DATE))?,
        within_date: get_key(frontmatter::WITHIN_DATE)
            .unwrap_or(Yaml::Integer(0))
            .into_i64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| anyhow!("invalid frontmatter value for {}", frontmatter::WITHIN_DATE))?,
    };

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
        sort_ordinal,
        body,
    })
}

fn main() {
    drop(dbg!(parse_article_file(Path::new(
        std::env::args_os().nth(1).expect("give arg").as_os_str()
    ))));
}
