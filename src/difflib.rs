//! Python `difflib.SequenceMatcher` and `get_close_matches`.
//!
//! Ratios are computed on Unicode scalar values, matching `str` iteration in
//! Python 3. `get_close_matches` sets sequence 2 to the word being matched.

use std::collections::{HashMap, HashSet};

fn calculate_ratio(matches: usize, length: usize) -> f64 {
    if length == 0 {
        1.0
    } else {
        2.0 * (matches as f64) / (length as f64)
    }
}

struct SequenceMatcher {
    a: Vec<char>,
    b: Vec<char>,
    b2j: HashMap<char, Vec<usize>>,
    bjunk: HashSet<char>,
    autojunk: bool,
    fullbcount: Option<HashMap<char, usize>>,
}

impl SequenceMatcher {
    fn new() -> Self {
        let mut s = Self {
            a: Vec::new(),
            b: Vec::new(),
            b2j: HashMap::new(),
            bjunk: HashSet::new(),
            autojunk: true,
            fullbcount: None,
        };
        s.chain_b();
        s
    }

    fn set_seq1(&mut self, a: &str) {
        self.a = a.chars().collect();
    }

    fn set_seq2(&mut self, b: &str) {
        self.b = b.chars().collect();
        self.fullbcount = None;
        self.chain_b();
    }

    fn chain_b(&mut self) {
        let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
        for (i, &elt) in self.b.iter().enumerate() {
            b2j.entry(elt).or_default().push(i);
        }
        // isjunk is None for get_close_matches, so nothing is purged as junk.
        self.bjunk.clear();
        if self.autojunk && self.b.len() >= 200 {
            let ntest = self.b.len() / 100 + 1;
            let popular: Vec<char> = b2j
                .iter()
                .filter(|(_, idxs)| idxs.len() > ntest)
                .map(|(&elt, _)| elt)
                .collect();
            for elt in popular {
                b2j.remove(&elt);
            }
        }
        self.b2j = b2j;
    }

    fn find_longest_match(&self, alo: usize, ahi: usize, blo: usize, bhi: usize) -> (usize, usize, usize) {
        let mut besti = alo;
        let mut bestj = blo;
        let mut bestsize = 0usize;
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for i in alo..ahi {
            let mut newj2len: HashMap<usize, usize> = HashMap::new();
            if let Some(indices) = self.b2j.get(&self.a[i]) {
                for &j in indices {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    // Python looks up j-1, which is -1 when j == 0 and misses.
                    let prev = if j == 0 {
                        0
                    } else {
                        j2len.get(&(j - 1)).copied().unwrap_or(0)
                    };
                    let k = prev + 1;
                    newj2len.insert(j, k);
                    if k > bestsize {
                        besti = i - k + 1;
                        bestj = j - k + 1;
                        bestsize = k;
                    }
                }
            }
            j2len = newj2len;
        }
        while besti > alo
            && bestj > blo
            && !self.bjunk.contains(&self.b[bestj - 1])
            && self.a[besti - 1] == self.b[bestj - 1]
        {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && !self.bjunk.contains(&self.b[bestj + bestsize])
            && self.a[besti + bestsize] == self.b[bestj + bestsize]
        {
            bestsize += 1;
        }
        while besti > alo
            && bestj > blo
            && self.bjunk.contains(&self.b[bestj - 1])
            && self.a[besti - 1] == self.b[bestj - 1]
        {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && self.bjunk.contains(&self.b[bestj + bestsize])
            && self.a[besti + bestsize] == self.b[bestj + bestsize]
        {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    }

    fn get_matching_blocks(&self) -> Vec<(usize, usize, usize)> {
        let la = self.a.len();
        let lb = self.b.len();
        let mut queue = vec![(0usize, la, 0usize, lb)];
        let mut matching_blocks = Vec::new();
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let (i, j, k) = self.find_longest_match(alo, ahi, blo, bhi);
            if k > 0 {
                matching_blocks.push((i, j, k));
                if alo < i && blo < j {
                    queue.push((alo, i, blo, j));
                }
                if i + k < ahi && j + k < bhi {
                    queue.push((i + k, ahi, j + k, bhi));
                }
            }
        }
        matching_blocks.sort_unstable();
        let mut i1 = 0usize;
        let mut j1 = 0usize;
        let mut k1 = 0usize;
        let mut non_adjacent = Vec::new();
        for &(i2, j2, k2) in &matching_blocks {
            if i1 + k1 == i2 && j1 + k1 == j2 {
                k1 += k2;
            } else {
                if k1 > 0 {
                    non_adjacent.push((i1, j1, k1));
                }
                i1 = i2;
                j1 = j2;
                k1 = k2;
            }
        }
        if k1 > 0 {
            non_adjacent.push((i1, j1, k1));
        }
        non_adjacent.push((la, lb, 0));
        non_adjacent
    }

    fn ratio(&self) -> f64 {
        let matches: usize = self.get_matching_blocks().iter().map(|t| t.2).sum();
        calculate_ratio(matches, self.a.len() + self.b.len())
    }

    fn quick_ratio(&mut self) -> f64 {
        if self.fullbcount.is_none() {
            let mut fullbcount: HashMap<char, usize> = HashMap::new();
            for &elt in &self.b {
                *fullbcount.entry(elt).or_insert(0) += 1;
            }
            self.fullbcount = Some(fullbcount);
        }
        let fullbcount = self.fullbcount.as_ref().unwrap();
        let mut avail: HashMap<char, isize> = HashMap::new();
        let mut matches = 0isize;
        for &elt in &self.a {
            let numb = if let Some(&n) = avail.get(&elt) {
                n
            } else {
                fullbcount.get(&elt).copied().unwrap_or(0) as isize
            };
            avail.insert(elt, numb - 1);
            if numb > 0 {
                matches += 1;
            }
        }
        calculate_ratio(matches as usize, self.a.len() + self.b.len())
    }

    fn real_quick_ratio(&self) -> f64 {
        let la = self.a.len();
        let lb = self.b.len();
        calculate_ratio(la.min(lb), la + lb)
    }
}

/// Python `difflib.get_close_matches`.
///
/// `n` must be > 0 and `cutoff` must be in `0.0..=1.0`. Returns the best
/// matches, highest ratio first. Equal ratios are broken by the possibility
/// string descending, matching `heapq.nlargest` on `(score, text)`.
pub fn get_close_matches(word: &str, possibilities: &[impl AsRef<str>], n: usize, cutoff: f64) -> Vec<String> {
    if n == 0 || !(0.0..=1.0).contains(&cutoff) {
        return Vec::new();
    }
    let mut matcher = SequenceMatcher::new();
    matcher.set_seq2(word);
    let mut result: Vec<(u64, String)> = Vec::new();
    for item in possibilities {
        let text = item.as_ref();
        matcher.set_seq1(text);
        // Split so the mutable quick_ratio borrow does not overlap ratio().
        let keep = matcher.real_quick_ratio() >= cutoff
            && matcher.quick_ratio() >= cutoff
            && matcher.ratio() >= cutoff;
        if keep {
            let score = matcher.ratio();
            // Positive ratios have monotonic IEEE bits. nlargest breaks ties
            // by the possibility string, descending.
            result.push((score.to_bits(), text.to_string()));
        }
    }
    result.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    result.into_iter().take(n).map(|(_, text)| text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python_close(word: &str, possibilities: &[&str], n: usize, cutoff: f64) -> Vec<String> {
        let list = possibilities
            .iter()
            .map(|p| format!("{:?}", p))
            .collect::<Vec<_>>()
            .join(", ");
        let code = format!(
            "import difflib, json; print(json.dumps(difflib.get_close_matches({word:?}, [{list}], n={n}, cutoff={cutoff})))"
        );
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(code)
            .output()
            .expect("python3");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    }

    #[test]
    fn matches_python_examples() {
        let cases: &[(&str, &[&str], usize, f64)] = &[
            ("appel", &["ape", "apple", "peach", "puppy"], 3, 0.6),
            ("brnch", &["branch", "status"], 1, 0.6),
            ("st", &["status", "reset"], 1, 0.6),
            ("vom", &["vim", "fsck", "git", "go", "python"], 3, 0.6),
            ("fucck", &["vim", "fsck", "git", "go", "python"], 3, 0.6),
            ("got", &["vim", "fsck", "git", "go", "python"], 3, 0.6),
            ("gti", &["vim", "fsck", "git", "go", "python"], 3, 0.6),
            ("puthon", &["python", "python3"], 3, 0.1),
            ("café", &["cafe", "café", "coffee"], 3, 0.6),
            ("йцукен", &["йцукенг", "qwerty", "йцук"], 3, 0.6),
            ("", &["a", ""], 3, 0.6),
            ("abc", &["abc", "ab", "abcd", "xyz"], 2, 0.6),
        ];
        for (word, poss, n, cutoff) in cases {
            let got = get_close_matches(word, poss, *n, *cutoff);
            let expect = python_close(word, poss, *n, *cutoff);
            assert_eq!(got, expect, "word={word:?} poss={poss:?} n={n} cutoff={cutoff}");
        }
    }

    #[test]
    fn ratio_matches_python_on_long_strings() {
        let a = "x".repeat(180) + &"y".repeat(40) + "return NULL;".repeat(30);
        let b = "x".repeat(180) + &"z".repeat(40) + "return NULL;".repeat(30);
        let code = format!(
            "import difflib; print(repr(difflib.SequenceMatcher(None, {a:?}, {b:?}).ratio()))"
        );
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(code)
            .output()
            .unwrap();
        let py: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        let mut m = SequenceMatcher::new();
        // get_close_matches compares possibility (seq1) against word (seq2).
        m.set_seq2(&b);
        m.set_seq1(&a);
        let rust = m.ratio();
        assert!((py - rust).abs() < 1e-12, "py={py} rust={rust}");
    }
}
