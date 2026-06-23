//! Custom-dictionary correction benchmark (issue #18).
//!
//! Asserts the precision-first fuzzy matcher makes ZERO false positives (never overwrites a
//! word it should not) and ZERO false negatives (always applies an expected correction) on a
//! labeled set. The cases are synthesized from real dictation errors and upstream bug reports
//! (Handy discussions #601, issues #18/#19); no personal transcript text is committed.
//!
//! Precision matters more than recall here: a missed correction is a minor annoyance, but a
//! wrong correction makes dictation feel unsafe. The deterministic replacement map (the
//! `clawed` -> `Claude` path) is exercised by the inline unit tests in `audio_toolkit::text`.
//!
//! Run with `cargo test --test dictionary_eval -- --nocapture` to see the metrics table.

use handy_app_lib::audio_toolkit::apply_custom_words;

/// Joe's real default; the matcher's per-length floors now dominate this dial.
const THRESHOLD: f64 = 0.18;

struct Case {
    name: &'static str,
    text: &'static str,
    custom_words: &'static [&'static str],
    /// Expected output. For true negatives this equals `text` (no change expected).
    expected: &'static str,
    /// True if a correction is expected (true positive); false if the input must be left alone.
    is_positive: bool,
}

const CASES: &[Case] = &[
    // ---- True positives: corrections the matcher SHOULD make ----
    Case {
        name: "typo: helo -> hello",
        text: "helo there",
        custom_words: &["hello"],
        expected: "hello there",
        is_positive: true,
    },
    Case {
        name: "transposition: wrold -> world",
        text: "the wrold is big",
        custom_words: &["world"],
        expected: "the world is big",
        is_positive: true,
    },
    Case {
        name: "ngram: co pilot -> Copilot",
        text: "use co pilot daily",
        custom_words: &["Copilot"],
        expected: "use Copilot daily",
        is_positive: true,
    },
    Case {
        name: "ngram: blue sky -> Bluesky",
        text: "posted on blue sky",
        custom_words: &["Bluesky"],
        expected: "posted on Bluesky",
        is_positive: true,
    },
    Case {
        name: "ngram: mac book -> MacBook",
        text: "my mac book pro",
        custom_words: &["MacBook"],
        expected: "my MacBook pro",
        is_positive: true,
    },
    Case {
        name: "ngram: Chat GPT -> ChatGPT",
        text: "ask Chat GPT please",
        custom_words: &["ChatGPT"],
        expected: "ask ChatGPT please",
        is_positive: true,
    },
    Case {
        name: "fuzzy ngram: Charge B -> ChargeBee",
        text: "the Charge B platform",
        custom_words: &["ChargeBee"],
        expected: "the ChargeBee platform",
        is_positive: true,
    },
    Case {
        name: "recase: codex -> Codex",
        text: "open codex now",
        custom_words: &["Codex"],
        expected: "open Codex now",
        is_positive: true,
    },
    Case {
        name: "ngram: Open AI -> OpenAI",
        text: "Open AI released a model",
        custom_words: &["OpenAI"],
        expected: "OpenAI released a model",
        is_positive: true,
    },
    Case {
        name: "typo: kubernetis -> Kubernetes",
        text: "deploy to kubernetis",
        custom_words: &["Kubernetes"],
        expected: "deploy to Kubernetes",
        is_positive: true,
    },
    Case {
        name: "ngram: git hub -> GitHub",
        text: "push to git hub",
        custom_words: &["GitHub"],
        expected: "push to GitHub",
        is_positive: true,
    },
    Case {
        name: "ngram: you tube -> YouTube",
        text: "watch on you tube",
        custom_words: &["YouTube"],
        expected: "watch on YouTube",
        is_positive: true,
    },
    // ---- True negatives: words the matcher MUST leave alone ----
    Case {
        name: "common word: cloud !-> Claude",
        text: "deployed to the cloud today",
        custom_words: &["Claude"],
        expected: "deployed to the cloud today",
        is_positive: false,
    },
    Case {
        name: "first-letter guard: region !-> Legion",
        text: "the region is down",
        custom_words: &["Legion"],
        expected: "the region is down",
        is_positive: false,
    },
    Case {
        name: "common word: working !-> Workday",
        text: "I was working late",
        custom_words: &["Workday"],
        expected: "I was working late",
        is_positive: false,
    },
    Case {
        name: "ngram length: work tree !-> Workday",
        text: "checked the work tree status",
        custom_words: &["Workday"],
        expected: "checked the work tree status",
        is_positive: false,
    },
    Case {
        name: "common word: really !-> rally",
        text: "that was really cool",
        custom_words: &["rally"],
        expected: "that was really cool",
        is_positive: false,
    },
    Case {
        name: "common word: nice !-> Niche",
        text: "this is a nice day",
        custom_words: &["Niche"],
        expected: "this is a nice day",
        is_positive: false,
    },
    Case {
        name: "short common word: run !-> Ruby",
        text: "let me run it",
        custom_words: &["Ruby"],
        expected: "let me run it",
        is_positive: false,
    },
    Case {
        name: "length: office !-> Officejawn",
        text: "back at the office now",
        custom_words: &["Officejawn"],
        expected: "back at the office now",
        is_positive: false,
    },
    Case {
        name: "length: house !-> Houseofjawn",
        text: "cleaning the house today",
        custom_words: &["Houseofjawn"],
        expected: "cleaning the house today",
        is_positive: false,
    },
    Case {
        name: "common word: ford !-> Forge",
        text: "drove the ford truck",
        custom_words: &["Forge"],
        expected: "drove the ford truck",
        is_positive: false,
    },
    Case {
        name: "length: data !-> Databricks",
        text: "the data looks good",
        custom_words: &["Databricks"],
        expected: "the data looks good",
        is_positive: false,
    },
    Case {
        name: "length: page !-> PagerDuty",
        text: "open the page now",
        custom_words: &["PagerDuty"],
        expected: "open the page now",
        is_positive: false,
    },
    Case {
        name: "common word: same !-> Sammy",
        text: "the same thing again",
        custom_words: &["Sammy"],
        expected: "the same thing again",
        is_positive: false,
    },
    Case {
        name: "length: links !-> LinkedIn",
        text: "share the links here",
        custom_words: &["LinkedIn"],
        expected: "share the links here",
        is_positive: false,
    },
];

#[test]
fn dictionary_zero_false_positives_and_negatives() {
    let owned: Vec<Vec<String>> = CASES
        .iter()
        .map(|c| c.custom_words.iter().map(|w| w.to_string()).collect())
        .collect();

    let (mut tp, mut fp, mut fn_, mut tn) = (0usize, 0usize, 0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();

    for (case, words) in CASES.iter().zip(owned.iter()) {
        let result = apply_custom_words(case.text, words, THRESHOLD);

        if case.is_positive {
            if result == case.expected {
                tp += 1;
            } else {
                fn_ += 1;
                failures.push(format!(
                    "FALSE NEGATIVE [{}]: got {:?}, expected {:?}",
                    case.name, result, case.expected
                ));
            }
        } else if result == case.text {
            tn += 1;
        } else {
            fp += 1;
            failures.push(format!(
                "FALSE POSITIVE [{}]: input {:?} was changed to {:?}",
                case.name, case.text, result
            ));
        }
    }

    let precision = ratio(tp, tp + fp);
    let recall = ratio(tp, tp + fn_);
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    let fpr = ratio(fp, fp + tn);

    println!(
        "\n=== Dictionary correction benchmark ({} cases) ===",
        CASES.len()
    );
    println!("TP={tp}  FP={fp}  FN={fn_}  TN={tn}");
    println!(
        "Precision={:.3}  Recall={:.3}  F1={:.3}  FPR={:.3}",
        precision, recall, f1, fpr
    );
    for line in &failures {
        println!("  {line}");
    }

    assert!(
        fp == 0 && fn_ == 0,
        "matcher must have zero false positives and zero false negatives; got fp={fp} fn={fn_}\n{}",
        failures.join("\n")
    );
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}
