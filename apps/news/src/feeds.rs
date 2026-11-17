//! The feeds you follow. Fixed for now (not editable on the Kindle); a
//! `news/feeds.json` in inkpad's data folder replaces this list if present:
//! `[{"category": "...", "name": "...", "url": "...", "about": "..."}]`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feed {
    pub category: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub about: String,
}

impl Feed {
    /// A stable id made from the name ("hacker-news-best").
    pub fn id(&self) -> String {
        let mut s = String::new();
        for c in self.name.chars() {
            if c.is_ascii_alphanumeric() {
                s.push(c.to_ascii_lowercase());
            } else if !s.ends_with('-') && !s.is_empty() {
                s.push('-');
            }
        }
        s.trim_end_matches('-').to_string()
    }
}

const DEFAULT: &[(&str, &str, &str, &str)] = &[
    ("Developer communities", "Hacker News", "https://news.ycombinator.com/rss", "Y Combinator's front page: startups, engineering, tech discussion."),
    ("Developer communities", "Hacker News Best", "https://hnrss.org/best", "Only the highest-voted Hacker News stories of the past few days."),
    ("Developer communities", "Show HN", "https://hnrss.org/show", "Developers launching side projects, tools and open-source software."),
    ("Developer communities", "Lobsters", "https://lobste.rs/rss", "Computing-focused links: systems programming, security, software engineering."),
    ("Developer communities", "TabNews", "https://www.tabnews.com.br/rss", "Brazilian developer community: programming, careers, articles in Portuguese."),
    ("Developer communities", "Slashdot", "https://rss.slashdot.org/Slashdot/slashdotMain", "News for nerds: open source, Linux, tech policy."),
    ("GitHub & open source", "GitHub Trending", "https://mshibanami.github.io/GitHubTrendingRSS/daily/all.xml", "Fastest-rising repositories today, all languages."),
    ("GitHub & open source", "GitHub Trending: Python", "https://mshibanami.github.io/GitHubTrendingRSS/daily/python.xml", "Trending Python repositories today."),
    ("GitHub & open source", "GitHub Trending: TypeScript", "https://mshibanami.github.io/GitHubTrendingRSS/daily/typescript.xml", "Trending TypeScript projects today."),
    ("GitHub & open source", "GitHub Trending: Rust", "https://mshibanami.github.io/GitHubTrendingRSS/daily/rust.xml", "Trending Rust tools and engines today."),
    ("GitHub & open source", "Changelog News", "https://changelog.com/news/feed", "Notable open-source releases and software news."),
    ("AI & machine learning", "r/LocalLLaMA Top Weekly", "https://www.reddit.com/r/LocalLLaMA/top/.rss?t=week", "The week's top posts on running open models locally."),
    ("AI & machine learning", "r/LocalLLaMA Hot", "https://www.reddit.com/r/LocalLLaMA/.rss", "What's trending on r/LocalLLaMA right now."),
    ("AI & machine learning", "Simon Willison", "https://simonwillison.net/atom/everything/", "Daily hands-on notes on new models, prompts and tools."),
    ("AI & machine learning", "Hugging Face Blog", "https://huggingface.co/blog/feed.xml", "Releases, tutorials and benchmarks for open AI models."),
    ("AI & machine learning", "Lil'Log", "https://lilianweng.github.io/index.xml", "Deep technical write-ups on agents, RL and diffusion."),
    ("AI & machine learning", "r/MachineLearning Top Weekly", "https://www.reddit.com/r/MachineLearning/top/.rss?t=week", "The week's papers, implementations and research news."),
    ("Hardware & DIY", "Hackaday", "https://hackaday.com/feed/", "Hardware hacks, reverse engineering, PCBs and builds."),
    ("Hardware & DIY", "CNX Software", "https://www.cnx-software.com/feed/", "Embedded systems, single-board computers, dev boards."),
    ("Hardware & DIY", "Adafruit Blog", "https://blog.adafruit.com/feed/", "Open hardware projects and electronics tutorials."),
    ("Hardware & DIY", "Phoronix", "https://www.phoronix.com/rss.php", "Linux kernel, open-source drivers, hardware benchmarks."),
    ("Tech journalism", "TecMundo", "https://rss.tecmundo.com.br/feed", "Consumer tech, software and science, in Portuguese."),
    ("Tech journalism", "Tecnoblog", "https://tecnoblog.net/feed/", "Brazilian tech journalism: gadgets, telecom, apps, reviews."),
    ("Tech journalism", "Ars Technica", "https://feeds.arstechnica.com/arstechnica/index", "Science, computing, security, space and tech policy."),
    ("Tech journalism", "The Verge", "https://www.theverge.com/rss/index.xml", "Hardware reviews, big tech and internet culture."),
    ("Tech journalism", "404 Media", "https://www.404media.co/rss/", "Investigative journalism on hacking, AI and the internet."),
    ("Tech journalism", "Techmeme", "https://www.techmeme.com/feed.xml", "The biggest breaking tech headlines."),
    ("Tech YouTube", "Marques Brownlee", "https://www.youtube.com/feeds/videos.xml?channel_id=UCBJycsmduvYEL83R_U4JriQ", "Consumer tech reviews: phones, EVs, industry commentary."),
    ("Tech YouTube", "Linus Tech Tips", "https://www.youtube.com/feeds/videos.xml?channel_id=UCXuqSBlHAE6Xw-yeJA0Tunw", "PC builds, homelab experiments, tech deep dives."),
    ("Tech YouTube", "Dave2D", "https://www.youtube.com/feeds/videos.xml?channel_id=UCVYamHliCI9rw1tHR1xbkfw", "Concise reviews of laptops, phones and portable tech."),
    ("Tech YouTube", "Canaltech", "https://www.youtube.com/feeds/videos.xml?channel_id=UC_bXJnsgwOqEPA_-6N6faKw", "Brazilian tech news and reviews, in Portuguese."),
    ("Tech YouTube", "Fireship", "https://www.youtube.com/feeds/videos.xml?channel_id=UCsBjURrPoezykLs9EqgamOA", "Fast developer explainers and weekly coding news."),
];

/// The feed list: `news/feeds.json` if you made one, else the built-in list.
pub fn load() -> Vec<Feed> {
    if let Some(list) = inkpad::ui::store::read_text("news/feeds.json").and_then(|s| serde_json::from_str::<Vec<Feed>>(&s).map_err(|e| eprintln!("news: feeds.json: {e}")).ok()) {
        if !list.is_empty() {
            return list;
        }
    }
    DEFAULT.iter().map(|(c, n, u, a)| Feed { category: (*c).into(), name: (*n).into(), url: (*u).into(), about: (*a).into() }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_are_unique_slugs() {
        let feeds = super::load();
        let mut ids: Vec<String> = feeds.iter().map(|f| f.id()).collect();
        assert_eq!(ids[0], "hacker-news");
        assert!(ids.contains(&"github-trending-python".to_string()));
        assert!(ids.contains(&"lil-log".to_string()));
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), feeds.len());
    }
}
