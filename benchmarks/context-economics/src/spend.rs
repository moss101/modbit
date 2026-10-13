//! The spend cap of the live evaluations (the `live-evals` workflow).
//!
//! A live run costs real money, so every live harness carries a
//! [`SpendMeter`]: it is charged from the usage the gateway returns (never
//! from an estimate of what a call should have cost), it refuses to let a new
//! call start once the cap is reached, and the run that was stopped by it is
//! reported as a partial result. A partial result is never a pass: the
//! harness reports which measurements are missing and exits non-zero.
//!
//! The cap is hard up to the calls in flight: the check happens before a call
//! starts, with a reserve for that call, so a run can overshoot by at most
//! the cost of the calls that were already running when the meter filled up.
//! Prices are USD per million tokens, in the same catalog syntax the product
//! reads (`MODBIT_<P>_MODELS`, for example
//! `glm-5.3-flash=0.15/0.50;ctx=200000`). Cached input is charged at the full
//! input price: the cap errs on the side of spending less.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// A model's list price, USD per million tokens.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Price {
    /// Input.
    pub input_per_mtok_usd: f64,
    /// Output.
    pub output_per_mtok_usd: f64,
}

impl Price {
    /// What `input` and `output` tokens cost.
    #[must_use]
    pub fn cost_usd(self, input_tokens: u64, output_tokens: u64) -> f64 {
        (input_tokens as f64 * self.input_per_mtok_usd
            + output_tokens as f64 * self.output_per_mtok_usd)
            / 1_000_000.0
    }
}

/// The price of `model` in a catalog spec (`model=in/out;key=value,...`).
///
/// # Errors
/// The spec does not list the model, or its price is not two numbers.
pub fn price_from_catalog(spec: &str, model: &str) -> Result<Price, String> {
    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let head = entry.split(';').next().unwrap_or_default();
        let Some((name, prices)) = head.split_once('=') else {
            continue;
        };
        if name.trim() != model {
            continue;
        }
        let (input, output) = prices
            .split_once('/')
            .ok_or_else(|| format!("{model}: expected `input/output` prices"))?;
        let number = |v: &str| {
            v.trim()
                .parse::<f64>()
                .ok()
                .filter(|p| p.is_finite() && *p >= 0.0)
                .ok_or_else(|| format!("{model}: price {v:?} is not a non-negative number"))
        };
        return Ok(Price {
            input_per_mtok_usd: number(input)?,
            output_per_mtok_usd: number(output)?,
        });
    }
    Err(format!(
        "the catalog does not list {model}: an unknown price is not free, so no cost can be charged"
    ))
}

/// The cap was reached: no new call may start.
#[derive(Clone, Debug, PartialEq)]
pub struct CapReached {
    /// The cap.
    pub cap_usd: f64,
    /// What had been spent.
    pub spent_usd: f64,
}

impl std::fmt::Display for CapReached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "spend cap reached: ${:.4} spent of ${:.4}; the run stops and reports what it has",
            self.spent_usd, self.cap_usd
        )
    }
}

impl std::error::Error for CapReached {}

#[derive(Debug, Default)]
struct Inner {
    spent_usd: f64,
    calls: u64,
    input_tokens: u64,
    output_tokens: u64,
    unknown_usage_calls: u64,
    stopped_by_cap: bool,
}

/// A running account of what a live run has spent.
#[derive(Debug)]
pub struct SpendMeter {
    cap_usd: f64,
    inner: Mutex<Inner>,
}

/// The account, as retained with a result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpendRecord {
    /// The cap the run was given.
    pub cap_usd: f64,
    /// What it spent, from the usage the gateway returned.
    pub spent_usd: f64,
    /// Calls charged.
    pub calls: u64,
    /// Input tokens charged.
    pub input_tokens: u64,
    /// Output tokens charged.
    pub output_tokens: u64,
    /// Calls whose reply carried no usage (their cost is unknown, not zero).
    pub unknown_usage_calls: u64,
    /// Whether the cap, not the work running out, ended the run.
    pub stopped_by_cap: bool,
}

impl SpendMeter {
    /// A meter with a cap.
    ///
    /// # Errors
    /// The cap is not a positive finite number.
    pub fn new(cap_usd: f64) -> Result<Self, String> {
        if !cap_usd.is_finite() || cap_usd <= 0.0 {
            return Err(format!(
                "the spend cap must be a positive number of dollars, got {cap_usd}"
            ));
        }
        Ok(Self {
            cap_usd,
            inner: Mutex::new(Inner::default()),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Charge a call from the usage its reply returned.
    pub fn charge(&self, price: Price, input_tokens: u64, output_tokens: u64) {
        self.charge_usd(
            price.cost_usd(input_tokens, output_tokens),
            input_tokens,
            output_tokens,
        );
    }

    /// Charge a cost already priced (a Core run's economics view).
    pub fn charge_usd(&self, usd: f64, input_tokens: u64, output_tokens: u64) {
        let mut i = self.lock();
        i.spent_usd += usd.max(0.0);
        i.calls += 1;
        i.input_tokens += input_tokens;
        i.output_tokens += output_tokens;
    }

    /// A call whose reply had no usage: counted, never priced at zero in
    /// silence.
    pub fn charge_unknown(&self) {
        let mut i = self.lock();
        i.calls += 1;
        i.unknown_usage_calls += 1;
    }

    /// May a call that could cost up to `reserve_usd` start? On refusal the
    /// meter remembers that the cap ended the run.
    ///
    /// # Errors
    /// The cap would be exceeded.
    pub fn admit(&self, reserve_usd: f64) -> Result<(), CapReached> {
        let mut i = self.lock();
        if i.spent_usd + reserve_usd.max(0.0) > self.cap_usd || i.spent_usd >= self.cap_usd {
            i.stopped_by_cap = true;
            return Err(CapReached {
                cap_usd: self.cap_usd,
                spent_usd: i.spent_usd,
            });
        }
        Ok(())
    }

    /// Dollars left before the cap.
    #[must_use]
    pub fn remaining_usd(&self) -> f64 {
        (self.cap_usd - self.lock().spent_usd).max(0.0)
    }

    /// The account so far.
    #[must_use]
    pub fn record(&self) -> SpendRecord {
        let i = self.lock();
        SpendRecord {
            cap_usd: self.cap_usd,
            spent_usd: i.spent_usd,
            calls: i.calls,
            input_tokens: i.input_tokens,
            output_tokens: i.output_tokens,
            unknown_usage_calls: i.unknown_usage_calls,
            stopped_by_cap: i.stopped_by_cap,
        }
    }
}

/// The retained outcome of a live evaluation: what the workflow checks before
/// it calls the run a pass.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveStatus {
    /// The row (`px135`, ...).
    pub row: String,
    /// Every required measurement is present.
    pub complete: bool,
    /// The required measurements that are missing, by name.
    pub missing: Vec<String>,
    /// The spend.
    pub spend: SpendRecord,
}

impl LiveStatus {
    /// A status from the spend and the list of missing measurements; the run
    /// is complete only when nothing is missing and the cap did not end it.
    #[must_use]
    pub fn new(row: &str, missing: Vec<String>, spend: SpendRecord) -> Self {
        let mut missing = missing;
        if spend.stopped_by_cap {
            missing.push(format!(
                "the spend cap (${:.2}) stopped the run before all work was done",
                spend.cap_usd
            ));
        }
        Self {
            row: row.to_owned(),
            complete: missing.is_empty(),
            missing,
            spend,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalog_price_is_read_and_an_unknown_model_has_no_price() {
        let spec = "glm-5.3-flash=0.15/0.50;ctx=200000;out=131072, other=1/2";
        let p = price_from_catalog(spec, "glm-5.3-flash").unwrap();
        assert!((p.cost_usd(1_000_000, 1_000_000) - 0.65).abs() < 1e-9);
        assert!(price_from_catalog(spec, "missing").is_err());
        assert!(price_from_catalog("m=1", "m").is_err());
    }

    #[test]
    fn the_meter_refuses_a_call_the_cap_cannot_cover_and_remembers_why() {
        let m = SpendMeter::new(1.0).unwrap();
        let p = Price {
            input_per_mtok_usd: 1.0,
            output_per_mtok_usd: 1.0,
        };
        assert!(m.admit(0.1).is_ok());
        m.charge(p, 450_000, 450_000);
        assert!(m.admit(0.05).is_ok());
        assert!(m.admit(0.2).is_err(), "0.90 + 0.20 would pass the cap");
        let r = m.record();
        assert!(r.stopped_by_cap && (r.spent_usd - 0.9).abs() < 1e-9);
        assert!(SpendMeter::new(0.0).is_err() && SpendMeter::new(f64::NAN).is_err());
        let s = LiveStatus::new("px999", vec![], r);
        assert!(!s.complete && s.missing.len() == 1, "{s:?}");
    }
}
