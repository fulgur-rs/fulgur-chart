//! Pure, deterministic memory-regression comparison shared by the `membench`
//! bench target and its unit tests. No disk I/O, no dhat — just data in/out.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Per-case allocation stats. `alloc_bytes` is the gated metric; `alloc_blocks`
/// is recorded for context. BTreeMap keys keep the baseline JSON stable-ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseStat {
    pub alloc_bytes: u64,
    pub alloc_blocks: u64,
}

/// case name -> stats.
pub type Baseline = BTreeMap<String, CaseStat>;

/// A flagged regression for one case (alloc_bytes exceeded the allowed factor).
#[derive(Debug, Clone)]
pub struct Regression {
    pub case: String,
    pub baseline: u64,
    pub current: u64,
    pub ratio: f64,
}

/// Returns the cases whose `alloc_bytes` exceeds `baseline * (1 + threshold_pct/100)`.
/// Only cases present in BOTH maps are compared (missing-baseline handling is the
/// caller's responsibility).
pub fn check(baseline: &Baseline, current: &Baseline, threshold_pct: f64) -> Vec<Regression> {
    let factor = 1.0 + threshold_pct / 100.0;
    let mut out = Vec::new();
    for (case, cur) in current {
        let Some(base) = baseline.get(case) else {
            continue;
        };
        let allowed = (base.alloc_bytes as f64) * factor;
        if (cur.alloc_bytes as f64) > allowed {
            let ratio = if base.alloc_bytes == 0 {
                f64::INFINITY
            } else {
                cur.alloc_bytes as f64 / base.alloc_bytes as f64
            };
            out.push(Regression {
                case: case.clone(),
                baseline: base.alloc_bytes,
                current: cur.alloc_bytes,
                ratio,
            });
        }
    }
    out
}
