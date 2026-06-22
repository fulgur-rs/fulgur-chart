//! Unit tests for the pure memory-regression comparison logic.
#[path = "../benches/membench_check.rs"]
mod membench_check;

use membench_check::{Baseline, CaseStat, check};

fn stat(bytes: u64) -> CaseStat {
    CaseStat {
        alloc_bytes: bytes,
        alloc_blocks: 0,
    }
}

fn baseline() -> Baseline {
    let mut b = Baseline::new();
    b.insert("a".to_string(), stat(1000));
    b.insert("b".to_string(), stat(2000));
    b
}

#[test]
fn no_regression_when_equal() {
    let regs = check(&baseline(), &baseline(), 25.0);
    assert!(regs.is_empty());
}

#[test]
fn within_threshold_is_ok() {
    let mut cur = baseline();
    // +24% on "a" (1000 -> 1240), threshold 25% => OK
    cur.insert("a".to_string(), stat(1240));
    let regs = check(&baseline(), &cur, 25.0);
    assert!(regs.is_empty(), "got {regs:?}");
}

#[test]
fn over_threshold_is_flagged() {
    let mut cur = baseline();
    // +30% on "a" (1000 -> 1300), threshold 25% => regression
    cur.insert("a".to_string(), stat(1300));
    let regs = check(&baseline(), &cur, 25.0);
    assert_eq!(regs.len(), 1);
    assert_eq!(regs[0].case, "a");
    assert_eq!(regs[0].current, 1300);
    assert_eq!(regs[0].baseline, 1000);
}

#[test]
fn improvement_is_not_flagged() {
    let mut cur = baseline();
    cur.insert("a".to_string(), stat(500)); // got better
    let regs = check(&baseline(), &cur, 25.0);
    assert!(regs.is_empty());
}

#[test]
fn current_case_missing_from_baseline_is_ignored_by_check() {
    // `check` only compares cases present in both; the membench main handles
    // "missing baseline" separately. A brand-new current case is not a
    // regression here.
    let mut cur = baseline();
    cur.insert("c".to_string(), stat(9999));
    let regs = check(&baseline(), &cur, 25.0);
    assert!(regs.is_empty());
}
