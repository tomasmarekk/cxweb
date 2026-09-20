//! First-attempt accounting. No retry can replace a failed observation.
use super::{Assessment, Category, cases};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    Timeout,
    RateLimited,
    Cancelled,
    Transport,
    WrongRoute,
    Interrupted,
}

pub enum Outcome {
    Response(Assessment),
    Failure(Failure),
}

enum State {
    NotStarted,
    Started,
    Finished(Outcome),
}

struct Entry {
    category: Category,
    adversarial: bool,
    custom_or_namespaced: bool,
    state: State,
}

pub struct Tally {
    entries: BTreeMap<String, Entry>,
}

#[derive(Default, Serialize)]
pub struct Counts {
    pub attempted: usize,
    pub completed: usize,
    pub protocol_valid: usize,
    pub task_passed: usize,
}

#[derive(Serialize)]
pub struct Report {
    pub planned: usize,
    pub counts: Counts,
    pub categories: BTreeMap<Category, Counts>,
    pub failures: BTreeMap<Failure, usize>,
    pub adversarial_attempted: usize,
    pub custom_or_namespaced_attempted: usize,
    pub complete: bool,
    pub protocol_wilson_95: Option<[f64; 2]>,
    pub meets_envelope_threshold: bool,
    /// Other native-client, route, safety and whole-task gates are independent.
    pub release_qualified: bool,
}

impl Default for Tally {
    fn default() -> Self {
        Self {
            entries: cases()
                .into_iter()
                .map(|case| {
                    (
                        case.id,
                        Entry {
                            category: case.category,
                            adversarial: case.adversarial,
                            custom_or_namespaced: case.custom_or_namespaced,
                            state: State::NotStarted,
                        },
                    )
                })
                .collect(),
        }
    }
}

impl Tally {
    /// Register an attempt before doing external work. Once started it stays in
    /// the denominator, even if no response arrives or the run is interrupted.
    pub fn begin(&mut self, id: &str) -> Result<(), &'static str> {
        let entry = self.entries.get_mut(id).ok_or("E_CORPUS_CASE")?;
        if !matches!(entry.state, State::NotStarted) {
            return Err("E_CORPUS_DUPLICATE_ATTEMPT");
        }
        entry.state = State::Started;
        Ok(())
    }

    pub fn finish(&mut self, id: &str, outcome: Outcome) -> Result<(), &'static str> {
        let entry = self.entries.get_mut(id).ok_or("E_CORPUS_CASE")?;
        if !matches!(entry.state, State::Started) {
            return Err("E_CORPUS_ATTEMPT_STATE");
        }
        if let Outcome::Response(result) = &outcome
            && result.task_passed
            && !result.protocol_valid
        {
            return Err("E_CORPUS_ASSESSMENT");
        }
        entry.state = State::Finished(outcome);
        Ok(())
    }

    pub fn report(&self) -> Report {
        let mut counts = Counts::default();
        let mut categories = BTreeMap::<Category, Counts>::new();
        let mut failures = BTreeMap::new();
        let mut adversarial_attempted = 0;
        let mut custom_or_namespaced_attempted = 0;
        for entry in self.entries.values() {
            let group = categories.entry(entry.category).or_default();
            if matches!(entry.state, State::NotStarted) {
                continue;
            }
            counts.attempted += 1;
            group.attempted += 1;
            adversarial_attempted += usize::from(entry.adversarial);
            custom_or_namespaced_attempted += usize::from(entry.custom_or_namespaced);
            if let State::Finished(outcome) = &entry.state {
                counts.completed += 1;
                group.completed += 1;
                match outcome {
                    Outcome::Response(result) => {
                        counts.protocol_valid += usize::from(result.protocol_valid);
                        group.protocol_valid += usize::from(result.protocol_valid);
                        counts.task_passed += usize::from(result.task_passed);
                        group.task_passed += usize::from(result.task_passed);
                    }
                    Outcome::Failure(failure) => *failures.entry(*failure).or_default() += 1,
                }
            }
        }
        let complete = counts.completed == self.entries.len();
        // Pending attempts stay in the denominator, but are not silently
        // classified as failures to manufacture a confidence interval.
        let protocol_wilson_95 = if counts.completed == counts.attempted {
            wilson(counts.protocol_valid, counts.attempted)
        } else {
            None
        };
        let meets_envelope_threshold =
            complete && counts.protocol_valid * 100 >= counts.attempted * 99;
        Report {
            planned: self.entries.len(),
            counts,
            categories,
            failures,
            adversarial_attempted,
            custom_or_namespaced_attempted,
            complete,
            protocol_wilson_95,
            meets_envelope_threshold,
            release_qualified: false,
        }
    }
}

fn wilson(successes: usize, total: usize) -> Option<[f64; 2]> {
    if total == 0 {
        return None;
    }
    let n = total as f64;
    let p = successes as f64 / n;
    let z = 1.959963984540054_f64;
    let divisor = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / divisor;
    let half = z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).sqrt() / divisor;
    Some([(center - half).max(0.0), (center + half).min(1.0)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(task_passed: bool) -> Outcome {
        Outcome::Response(Assessment {
            protocol_valid: true,
            task_passed,
            failure: None,
        })
    }

    #[test]
    fn first_attempts_cannot_be_replaced_and_unfinished_work_remains_counted() {
        let mut tally = Tally::default();
        assert_eq!(tally.report().protocol_wilson_95, None);
        assert_eq!(tally.begin("unknown"), Err("E_CORPUS_CASE"));
        assert_eq!(
            tally.finish("case-001", valid(true)),
            Err("E_CORPUS_ATTEMPT_STATE")
        );
        tally.begin("case-001").unwrap();
        assert_eq!(tally.begin("case-001"), Err("E_CORPUS_DUPLICATE_ATTEMPT"));
        let report = tally.report();
        assert_eq!((report.counts.attempted, report.counts.completed), (1, 0));
        assert_eq!(report.protocol_wilson_95, None);
        assert!(!report.complete && !report.meets_envelope_threshold);
        tally
            .finish("case-001", Outcome::Failure(Failure::Timeout))
            .unwrap();
        assert_eq!(tally.begin("case-001"), Err("E_CORPUS_DUPLICATE_ATTEMPT"));
        assert_eq!(
            tally.finish("case-001", valid(true)),
            Err("E_CORPUS_ATTEMPT_STATE")
        );
        tally.begin("case-002").unwrap();
        tally.finish("case-002", valid(false)).unwrap();
        let report = tally.report();
        assert_eq!((report.counts.attempted, report.counts.completed), (2, 2));
        assert_eq!(
            (report.counts.protocol_valid, report.counts.task_passed),
            (1, 0)
        );
        assert_eq!(report.failures[&Failure::Timeout], 1);
    }

    #[test]
    fn complete_sample_reports_exact_denominators_without_claiming_release() {
        let mut tally = Tally::default();
        for (index, case) in cases().iter().enumerate() {
            tally.begin(&case.id).unwrap();
            let outcome = match index {
                0 => Outcome::Failure(Failure::RateLimited),
                1 => Outcome::Response(Assessment {
                    protocol_valid: false,
                    task_passed: false,
                    failure: Some("E_TOOL_ENVELOPE_JSON"),
                }),
                2 => valid(false), // protocol-valid refusal is not task success
                _ => valid(true),
            };
            tally.finish(&case.id, outcome).unwrap();
        }
        let report = tally.report();
        assert!(report.complete && report.meets_envelope_threshold);
        assert_eq!(
            (report.counts.attempted, report.counts.completed),
            (200, 200)
        );
        assert_eq!(
            (report.counts.protocol_valid, report.counts.task_passed),
            (198, 197)
        );
        assert_eq!(report.adversarial_attempted, 40);
        assert_eq!(report.custom_or_namespaced_attempted, 60);
        assert!(
            report
                .categories
                .values()
                .all(|group| group.attempted == 20)
        );
        assert!(!report.release_qualified);
        let interval = report.protocol_wilson_95.unwrap();
        assert!((interval[0] - 0.964279).abs() < 0.00001);
        assert!((interval[1] - 0.997253).abs() < 0.00001);
    }
}
