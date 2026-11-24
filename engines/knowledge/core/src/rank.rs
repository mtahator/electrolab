//! Ranking by words: papers by their digests, places by which words meet there.

use std::collections::{HashMap, HashSet};

use ep_knowledge_contract::{Digest, Kind};

use crate::{content, is_stop, squash, stem, words};

/// Words of a question, with the phrases a model added; no doubles, at most `most`.
#[must_use]
pub fn terms(question: &str, more: &[String], most: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let own: Vec<String> = question
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty() && !is_stop(&w.to_lowercase()))
        .map(str::to_string)
        .collect();
    more.iter()
        .map(|t| squash(t))
        .filter(|t| {
            (2..=48).contains(&t.chars().count())
                && t.split(' ').count() <= 4
                && content(t).next().is_some()
        })
        .chain(own)
        .filter(|t| {
            let key = t.split([' ', '-']).map(stem).collect::<Vec<_>>().join(" ");
            seen.insert(key)
        })
        .take(most)
        .collect()
}

/// What a question looks for, by its own words.
#[must_use]
pub fn asked_for(question: &str) -> Option<Kind> {
    words(question).find_map(|w| match w.as_str() {
        "figure" | "figures" | "fig" | "chart" | "charts" | "graph" | "graphs" | "plot"
        | "plots" | "diagram" | "curve" | "image" | "picture" | "نمودار" | "شکل" => {
            Some(Kind::Figure)
        }
        "table" | "tables" | "جدول" => Some(Kind::Table),
        "equation" | "equations" | "formula" | "formulas" | "معادله" => Some(Kind::Equation),
        _ => None,
    })
}

/// The words of a digest with what they weigh.
fn weighed(d: &Digest) -> Vec<(f32, String)> {
    let mut v = vec![
        (4.0, d.title.clone()),
        (3.0, d.topics.join(" ")),
        (2.0, d.summary.clone()),
        (2.0, d.questions.join(" ")),
    ];
    for s in &d.sections {
        v.push((1.5, s.title.clone()));
        v.push((2.0, s.tags.join(" ")));
        v.push((1.5, s.terms.join(" ")));
        v.push((1.0, s.summary.clone()));
    }
    for o in &d.objects {
        v.push((1.5, o.caption.clone()));
    }
    v
}

/// Score of every digest for these terms (BM25-like; rare words weigh more).
#[must_use]
#[allow(clippy::cast_precision_loss)] // counts
pub fn papers(digests: &[Digest], terms: &[String]) -> Vec<f32> {
    let want: Vec<Vec<String>> = terms.iter().map(|t| content(t).collect()).collect();
    let docs: Vec<HashMap<String, f32>> = digests
        .iter()
        .map(|d| {
            let mut m: HashMap<String, f32> = HashMap::new();
            for (w, text) in weighed(d) {
                for x in content(&text) {
                    *m.entry(x).or_insert(0.0) += w;
                }
            }
            m
        })
        .collect();
    let n = docs.len().max(1) as f32;
    let rare = |w: &String| -> f32 {
        let have = docs.iter().filter(|d| d.contains_key(w)).count() as f32;
        (1.0 + (n - have + 0.5) / (have + 0.5)).ln()
    };
    let rarity: HashMap<&String, f32> = want.iter().flatten().map(|w| (w, rare(w))).collect();
    docs.iter()
        .map(|d| {
            want.iter()
                .filter(|t| !t.is_empty())
                .map(|t| {
                    let each: Vec<f32> = t
                        .iter()
                        .map(|w| {
                            d.get(w)
                                .map_or(0.0, |c| rarity[w] * c * 2.2 / (c + 1.2 * 3.0))
                        })
                        .collect();
                    // A phrase counts by its weakest word.
                    each.iter().copied().fold(f32::INFINITY, f32::min) * t.len() as f32
                })
                .sum()
        })
        .collect()
}

/// Every word of the digests.
#[must_use]
pub fn vocabulary(digests: &[Digest]) -> HashSet<String> {
    digests
        .iter()
        .flat_map(weighed)
        .flat_map(|(_, t)| content(&t).collect::<Vec<_>>())
        .collect()
}

/// The written-out forms of abbreviations among the terms (`NPV` → `net present value`), as the papers
/// write them: the words in a row with these first letters that stand there most, at least twice.
#[must_use]
pub fn written_out(terms: &[String], digests: &[Digest]) -> Vec<String> {
    let short: Vec<String> = terms
        .iter()
        .filter(|t| (3..=5).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphabetic()))
        .filter(|t| t.chars().all(|c| c.is_ascii_uppercase()))
        .map(|t| t.to_lowercase())
        .collect();
    let mut out: Vec<String> = vec![];
    if short.is_empty() {
        return out;
    }
    let mut count: HashMap<(usize, String), u32> = HashMap::new();
    for (_, text) in digests.iter().flat_map(weighed) {
        // Words of one phrase: nothing but a space or a hyphen between them.
        for part in text.split(|c: char| !(c.is_alphanumeric() || c == ' ' || c == '-')) {
            let w: Vec<String> = part
                .split([' ', '-'])
                .filter(|x| !x.is_empty())
                .map(str::to_lowercase)
                .collect();
            for (n, s) in short.iter().enumerate() {
                for run in w.windows(s.len()) {
                    let fits = run
                        .iter()
                        .zip(s.chars())
                        .all(|(x, c)| x.starts_with(c) && x.chars().count() > 2 && !is_stop(x));
                    if fits {
                        *count.entry((n, run.join(" "))).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    for n in 0..short.len() {
        let best = count
            .iter()
            .filter(|((m, _), c)| *m == n && **c > 1)
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.1.cmp(&a.0.1)));
        out.extend(best.map(|((_, p), _)| p.clone()));
    }
    out
}

/// Is a term in these words?
#[must_use]
#[allow(clippy::implicit_hasher)] // used with the standard one
pub fn stands_in(term: &str, words: &HashSet<String>) -> bool {
    content(term).any(|w| words.contains(&w))
}

/// Is a term written in Latin letters (or digits)?
#[must_use]
pub fn latin(term: &str) -> bool {
    term.chars()
        .filter(|c| c.is_alphabetic())
        .all(|c| (c as u32) < 0x0250)
}

/// Which terms stand in a text.
#[must_use]
pub fn met(terms: &[String], text: &str) -> Vec<String> {
    let have: HashSet<String> = content(text).collect();
    terms
        .iter()
        .filter(|t| {
            let mut w = content(t).peekable();
            w.peek().is_some() && w.all(|x| have.contains(&x))
        })
        .cloned()
        .collect()
}

/// Things a model put together (`carbon saving against NPV`) taken apart, name by name.
#[must_use]
pub fn apart(things: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let parts = |name: &str| -> Vec<String> {
        let mut out = vec![String::new()];
        for w in name.split_whitespace() {
            let between = matches!(
                w.trim_matches('.').to_lowercase().as_str(),
                "vs" | "versus" | "against" | "and" | "compared" | "with" | "to"
            );
            if between {
                out.push(String::new());
            } else if let Some(p) = out.last_mut() {
                if !p.is_empty() {
                    p.push(' ');
                }
                p.push_str(w);
            }
        }
        out.retain(|p| !p.is_empty());
        out
    };
    let mut out: Vec<Vec<String>> = vec![];
    for names in things {
        let mut split: Vec<Vec<String>> = vec![];
        for name in &names {
            for (n, p) in parts(name).into_iter().enumerate() {
                if split.len() <= n {
                    split.push(vec![]);
                }
                if !split[n].contains(&p) {
                    split[n].push(p);
                }
            }
        }
        // `power to gas` is one thing when no other name is put together the same way.
        let together = names.iter().filter(|n| parts(n).len() > 1).count();
        if split.len() > 1 && together * 2 >= names.len() {
            out.extend(split);
        } else {
            out.push(names);
        }
    }
    out.truncate(6);
    out
}

/// Share of the things looked for that are named, each by any of its names (0–1).
#[must_use]
#[allow(clippy::cast_precision_loss)] // a few things
pub fn coverage(things: &[Vec<String>], met: &[String]) -> f32 {
    if things.is_empty() {
        return 0.0;
    }
    let named = things
        .iter()
        .filter(|names| names.iter().any(|n| met.contains(n)))
        .count();
    named as f32 / things.len() as f32
}

/// Score of a place: how many of the things are named, how rare the words are, whether it is the kind
/// asked for.
#[must_use]
#[allow(clippy::implicit_hasher)] // used with the standard one
pub fn place(
    met: &[String],
    rarity: &HashMap<String, f32>,
    things: &[Vec<String>],
    kind: Kind,
    asked: Kind,
) -> f32 {
    let mut s: f32 = met
        .iter()
        .map(|t| rarity.get(t).copied().unwrap_or(1.0))
        .sum();
    // Things that meet count more than one thing often, or many names of one thing.
    s = s.sqrt() * (0.3 + 3.0 * coverage(things, met));
    let labelled = matches!(asked, Kind::Figure | Kind::Table | Kind::Equation);
    if labelled && kind == asked {
        s * 2.0
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ep_knowledge_contract::{ObjectDigest, SectionDigest};

    fn digest(title: &str, topics: &[&str], caption: &str) -> Digest {
        Digest {
            title: title.into(),
            topics: topics.iter().map(|t| (*t).into()).collect(),
            sections: vec![SectionDigest {
                title: "Introduction".into(),
                ..SectionDigest::default()
            }],
            objects: vec![ObjectDigest {
                kind: Kind::Figure,
                label: "Figure 1".into(),
                page: 3,
                y: 0.0,
                object: "o".into(),
                rect: None,
                caption: caption.into(),
            }],
            ..Digest::default()
        }
    }

    #[test]
    fn the_paper_about_it_comes_first() {
        let d = [
            digest(
                "Turbulence near a wing tip",
                &["vortex"],
                "Velocity contours",
            ),
            digest(
                "A renewable power system for a campus",
                &["renewable energy"],
                "Avoided carbon compared with net present value",
            ),
            digest("Invoice 12", &["invoice"], ""),
        ];
        let t = terms(
            "chart of carbon saving and NPV for the different technologies",
            &["net present value".into(), "avoided carbon".into()],
            10,
        );
        assert_eq!(t[..2], ["net present value", "avoided carbon"]);
        assert!(t.contains(&"NPV".to_string()) && !t.iter().any(|x| x == "the" || x == "chart"));
        let s = papers(&d, &t);
        assert!(s[1] > s[0] && s[1] > s[2], "{s:?}");
        assert_eq!(asked_for("a chart of NPV"), Some(Kind::Figure));
    }

    #[test]
    fn abbreviations_are_written_out_as_the_papers_do() {
        let d = [
            digest(
                "Net present value of a campus",
                &[],
                "Net-present value over the years",
            ),
            digest("New plans, very old", &[], "new pumps valves"),
        ];
        let t = ["NPV".to_string(), "carbon".to_string(), "IRR".to_string()];
        assert_eq!(written_out(&t, &d), ["net present value"]);
    }

    #[test]
    fn things_put_together_are_taken_apart() {
        let v = |x: &[&str]| x.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let t = apart(vec![
            v(&["carbon saving against NPV", "carbon abatement vs NPV"]),
            v(&["hydrogen", "power to gas", "electrolysis"]),
        ]);
        assert_eq!(t[0], ["carbon saving", "carbon abatement"]);
        assert_eq!(t[1], ["NPV"]);
        assert_eq!(t[2], ["hydrogen", "power to gas", "electrolysis"]);
    }

    #[test]
    fn words_of_another_script_only_count_where_they_are_used() {
        let used = vocabulary(&[digest("Carbon of a campus", &[], "")]);
        assert!(latin("CO2") && latin("Müller") && !latin("نمودار"));
        assert!(stands_in("carbon saving", &used) && !stands_in("نمودار", &used));
    }

    #[test]
    fn a_place_where_words_meet_comes_first() {
        let t: Vec<String> = ["carbon", "npv", "payback"].map(String::from).into();
        let r: HashMap<String, f32> = t.iter().map(|x| (x.clone(), 1.0)).collect();
        let both = met(&t, "NPV and avoided carbons");
        assert_eq!(both, ["carbon", "npv"]);
        let one = met(&t, "payback payback payback");
        let things: Vec<Vec<String>> = t.iter().map(|x| vec![x.clone()]).collect();
        let score = |m: &[String], k, asked| place(m, &r, &things, k, asked);
        assert!(score(&both, Kind::Text, Kind::Text) > 2.0 * score(&one, Kind::Text, Kind::Text));
        assert!(score(&both, Kind::Figure, Kind::Figure) > score(&both, Kind::Text, Kind::Figure));
        // Two names of one thing are not two things.
        let two = vec![
            vec!["carbon".to_string(), "npv".to_string()],
            vec!["payback".to_string()],
        ];
        assert!((coverage(&two, &both) - 0.5).abs() < 1e-6);
    }
}
