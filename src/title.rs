use regex::Regex;
use std::sync::LazyLock;

// Pre-compiled regexes for efficient title cleaning
static RE_SEASON_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S(\d{2})E(\d{2})").unwrap());
static RE_FILE_EXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.[a-zA-Z0-9]{2,4}$").unwrap());
static RE_BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\[\(\{][^\]\)\}]*[\]\)\}]").unwrap());
static RE_AFTER_YEAR_DOTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.((?:19|20)\d{2})\..*").unwrap());
static RE_YEAR_STANDALONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b((?:19|20)\d{2})\b").unwrap());
static RE_QUALITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(720p|1080p|2160p|4k|uhd|bluray|blu[\.\-]?ray|brrip|bdrip|webrip|web[\.\-]?dl|webdl|hdtv|hdrip|dvdrip|dvdscr|x\.?264|x\.?265|h\.?264|h\.?265|hevc|avc|aac|ac3|dts|atmos|10bit|hdr10?|sdr|remux|proper|repack|extended|unrated|directors?\.?cut|theatrical|imax)\b").unwrap()
});
static RE_RELEASE_GROUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\-\s](yts|yify|rarbg|eztv|ettv|sparks|geckos|fleet|ntb|evo|fgt|ion10|strife|megusta|mkvkids|t[ao]p|tigole|qxr|psyence|joy|mk(?:vcage)?)\b").unwrap()
});
static RE_DOTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.").unwrap());
static RE_MULTI_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());
static RE_SEPARATORS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[_\|]+").unwrap());
static RE_COPYRIGHT_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\|\-]+\s?").unwrap());

// Result of parsing a media title
#[derive(Debug)]
pub struct CleanedTitle {
    pub name: String,
    pub year: Option<u32>,
    pub season_episode: Option<(u32, u32)>,
}

// Clean a media filename/title to extract the actual title, year, and season/episode info.
pub fn clean_title(raw_title: &str) -> CleanedTitle {
    let title = raw_title.trim();

    // Check for TV show pattern (SxxExx)
    if let Some(caps) = RE_SEASON_EPISODE.captures(title) {
        let season: u32 = caps[1].parse().unwrap_or(0);
        let episode: u32 = caps[2].parse().unwrap_or(0);

        // Everything before SxxExx is the show name
        let se_start = caps.get(0).unwrap().start();
        let before_se = &title[..se_start];

        let (name, year) = clean_name(before_se);

        return CleanedTitle {
            name,
            year,
            season_episode: Some((season, episode)),
        };
    }

    // Movie / regular media
    let (name, year) = clean_name(title);

    CleanedTitle {
        name,
        year,
        season_episode: None,
    }
}

// Internal: clean a name string by removing common filename junk
fn clean_name(raw: &str) -> (String, Option<u32>) {
    let mut name = raw.to_string();

    // Remove file extension
    name = RE_FILE_EXT.replace_all(&name, "").to_string();

    // Extract year from dot-separated names and remove everything after it
    // e.g. Movie.2024.1080p.BluRay -> year=2024, name=Movie
    let mut year: Option<u32> = None;
    if let Some(caps) = RE_AFTER_YEAR_DOTS.captures(&name) {
        year = caps[1].parse().ok();
        name = RE_AFTER_YEAR_DOTS.replace_all(&name, "").to_string();
    }

    // Remove bracketed content: [stuff] (stuff) {stuff}
    name = RE_BRACKETS.replace_all(&name, "").to_string();

    // Remove quality/codec tags
    name = RE_QUALITY.replace_all(&name, "").to_string();

    // Remove release group names
    name = RE_RELEASE_GROUP.replace_all(&name, "").to_string();

    // Replace dots with spaces
    name = RE_DOTS.replace_all(&name, " ").to_string();

    // Replace underscores and pipes with spaces
    name = RE_SEPARATORS.replace_all(&name, " ").to_string();

    // Remove copyright-style prefix (e.g., "|- Title")
    name = RE_COPYRIGHT_PREFIX.replace_all(&name, "").to_string();

    // Extract year if not already found (from space-separated format)
    if year.is_none() {
        if let Some(caps) = RE_YEAR_STANDALONE.captures(&name) {
            year = caps[1].parse().ok();
            // Remove the year from the title for cleaner TMDB search
            name = RE_YEAR_STANDALONE.replace(&name, "").to_string();
        }
    }

    // Collapse multiple spaces
    name = RE_MULTI_SPACES.replace_all(&name, " ").to_string();

    // Remove trailing hyphens/dashes (common separator before quality info)
    name = name.trim().trim_end_matches('-').trim().to_string();

    (name, year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_movie_with_year_and_quality() {
        let r = clean_title("The.Matrix.1999.1080p.BluRay.x264-SPARKS.mkv");
        assert_eq!(r.name, "The Matrix");
        assert_eq!(r.year, Some(1999));
        assert!(r.season_episode.is_none());
    }

    #[test]
    fn test_tv_show() {
        let r = clean_title("Breaking.Bad.S05E16.720p.BluRay.mkv");
        assert_eq!(r.name, "Breaking Bad");
        assert_eq!(r.season_episode, Some((5, 16)));
    }

    #[test]
    fn test_simple_title() {
        let r = clean_title("Inception");
        assert_eq!(r.name, "Inception");
        assert!(r.season_episode.is_none());
    }

    #[test]
    fn test_bracketed_info() {
        let r = clean_title("[YTS] Movie Name (2023) [1080p].mkv");
        assert_eq!(r.name, "Movie Name");
        assert!(r.season_episode.is_none());
    }

    #[test]
    fn test_tv_with_dots() {
        let r = clean_title("The.Office.S02E03.Episode.Title.720p.mkv");
        assert_eq!(r.name, "The Office");
        assert_eq!(r.season_episode, Some((2, 3)));
    }
}
