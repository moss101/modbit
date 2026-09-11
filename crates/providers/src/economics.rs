//! Cache economics and routing epochs (REQ-EPR-009; docs/27 §7.6 and
//! §16.3, docs/38 "CompileConfidenceFeasiblePlan" step 6): what switching
//! away from the binding in force would cost once the cached prefix, the
//! re-prefill, the cache write, the switch latency and a hysteresis margin
//! are counted, on one accounting basis with nothing counted twice.
//!
//! Integer minor units throughout; the conversion of latency into money is
//! an explicit input (`latency_minor_per_ms`), as is the hysteresis margin
//! (basis points of the stay cost). A nominally cheaper model may be more
//! expensive after a cache miss; this module says by how much.

use serde::{Deserialize, Serialize};

use crate::registry::RegistryEntry;

/// Economics rules version.
pub const ECONOMICS_VERSION: &str = "economics-1";

/// Default cache lifetime when the registry entry names none (the common
/// provider default).
pub const DEFAULT_CACHE_TTL_MS: u64 = 300_000;

/// The cached prefix a session holds with the binding in force.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheState {
    /// Endpoint.
    pub endpoint: String,
    /// Model.
    pub model: String,
    /// Tokens the provider last reported as served from cache.
    pub cached_prefix_tokens: u64,
    /// When the prefix was last used (millis since epoch).
    pub last_used_at_ms: i64,
    /// The prefix's identity (the request cache key), for the audit.
    pub prefix_key: String,
}

impl CacheState {
    /// Whether the prefix is still warm at `now_ms` under `ttl_ms`.
    #[must_use]
    pub fn warm_at(&self, now_ms: i64, ttl_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_used_at_ms) <= ttl_ms as i64
    }
}

/// What the rest of the transaction is expected to consume.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemainingDemand {
    /// Input tokens per remaining call, of which the cached prefix is a part.
    pub input_tokens_per_call: u64,
    /// Output tokens per remaining call.
    pub output_tokens_per_call: u64,
    /// Calls still expected.
    pub calls: u64,
}

/// The switch cost, itemized (docs/27 §7.6).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchCost {
    /// What the warm prefix is worth to the binding in force over the
    /// remaining calls: the discount between its input and cached-input
    /// prices. Informational — it is the reason `re_prefill_minor` exists,
    /// never added on top of it.
    pub lost_cache_value_minor: u64,
    /// What the alternative pays to prefill the prefix the current binding
    /// would have read from cache, over the remaining calls, beyond what
    /// staying pays for the same tokens.
    pub re_prefill_minor: u64,
    /// The alternative's cache write for the prefix, once.
    pub cache_write_minor: u64,
    /// The latency the switch adds, priced.
    pub latency_penalty_minor: u64,
    /// The margin a switch must clear on top (basis points of the stay cost).
    pub hysteresis_minor: u64,
    /// Sum of the four priced items (not the informational one).
    pub total_minor: u64,
    /// Whether the prefix was warm at evaluation time.
    pub prefix_warm: bool,
}

/// Stay or switch, with both totals on the same basis.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaySwitch {
    /// Version.
    pub economics_version: String,
    /// Staying: the remaining demand at the current binding's prices with
    /// the warm prefix read from cache.
    pub stay_minor: u64,
    /// Switching: the remaining demand at the alternative's prices, the
    /// switch cost included.
    pub switch_minor: u64,
    /// The itemized switch cost.
    pub switch_cost: SwitchCost,
    /// `STAY` | `SWITCH`.
    pub decision: String,
    /// Why, in words.
    pub reason: String,
}

/// Cost of `tokens` at `price_per_mtok_minor`, rounded up.
#[must_use]
pub fn cost_minor(tokens: u64, price_per_mtok_minor: u64) -> u64 {
    (u128::from(tokens) * u128::from(price_per_mtok_minor)).div_ceil(1_000_000) as u64
}

/// Compare staying on `current` with switching to `alternative` for the
/// remaining demand. `cache` is the session's warm prefix, when one exists
/// for `current`; `now_ms` decides whether it is still warm.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn compare(
    current: &RegistryEntry,
    alternative: &RegistryEntry,
    cache: Option<&CacheState>,
    now_ms: i64,
    demand: RemainingDemand,
    latency_minor_per_ms: u64,
    hysteresis_bp: u64,
) -> StaySwitch {
    let calls = demand.calls.max(1);
    let cur_in = current.economics.input_per_mtok_minor;
    let cur_cached = current
        .economics
        .cached_input_per_mtok_minor
        .unwrap_or(cur_in)
        .min(cur_in);
    let cur_out = current.economics.output_per_mtok_minor;
    let alt_in = alternative.economics.input_per_mtok_minor;
    let alt_out = alternative.economics.output_per_mtok_minor;
    let alt_write = alternative
        .economics
        .cache_write_per_mtok_minor
        .unwrap_or(0);
    let ttl = current
        .economics
        .cache_ttl_ms
        .unwrap_or(DEFAULT_CACHE_TTL_MS);
    let warm = cache.is_some_and(|c| {
        c.endpoint == current.endpoint && c.model == current.model && c.warm_at(now_ms, ttl)
    });
    let prefix = if warm {
        cache
            .map(|c| c.cached_prefix_tokens)
            .unwrap_or(0)
            .min(demand.input_tokens_per_call)
    } else {
        0
    };
    let uncached_in = demand.input_tokens_per_call.saturating_sub(prefix);
    // Stay: prefix from cache, the rest at full price, every remaining call.
    let stay_per_call = cost_minor(prefix, cur_cached)
        + cost_minor(uncached_in, cur_in)
        + cost_minor(demand.output_tokens_per_call, cur_out);
    let stay = stay_per_call.saturating_mul(calls);
    // Switch: everything at the alternative's prices; the prefix is paid in
    // full there — that is the re-prefill, counted once as the difference
    // between what the alternative pays for those tokens and what staying
    // would have paid for them from cache.
    let alt_per_call = cost_minor(demand.input_tokens_per_call, alt_in)
        + cost_minor(demand.output_tokens_per_call, alt_out);
    let alt_total = alt_per_call.saturating_mul(calls);
    let re_prefill = cost_minor(prefix, alt_in)
        .saturating_sub(cost_minor(prefix, cur_cached))
        .saturating_mul(calls);
    let lost_cache_value = cost_minor(prefix, cur_in)
        .saturating_sub(cost_minor(prefix, cur_cached))
        .saturating_mul(calls);
    let cache_write = cost_minor(prefix, alt_write);
    let latency_penalty = alternative
        .latency
        .p50_ms
        .saturating_sub(current.latency.p50_ms)
        .saturating_mul(latency_minor_per_ms);
    let hysteresis = (u128::from(stay) * u128::from(hysteresis_bp) / 10_000) as u64;
    let switch_cost = SwitchCost {
        lost_cache_value_minor: lost_cache_value,
        re_prefill_minor: re_prefill,
        cache_write_minor: cache_write,
        latency_penalty_minor: latency_penalty,
        hysteresis_minor: hysteresis,
        total_minor: re_prefill + cache_write + latency_penalty + hysteresis,
        prefix_warm: warm,
    };
    // `alt_total` already pays the prefix at the alternative's input price;
    // the re-prefill is inside it. The switch total adds only what is not:
    // the cache write, the latency and the hysteresis margin.
    let switch_total = alt_total + cache_write + latency_penalty + hysteresis;
    let (decision, reason) = if switch_total < stay {
        (
            "SWITCH",
            format!(
                "switching saves {} minor units after a switch cost of {} (prefix {} tokens, warm: {warm})",
                stay - switch_total,
                switch_cost.total_minor,
                prefix
            ),
        )
    } else {
        (
            "STAY",
            format!(
                "staying is cheaper by {} minor units once the switch cost of {} is counted (prefix {} tokens, warm: {warm})",
                switch_total - stay,
                switch_cost.total_minor,
                prefix
            ),
        )
    };
    StaySwitch {
        economics_version: ECONOMICS_VERSION.into(),
        stay_minor: stay,
        switch_minor: switch_total,
        switch_cost,
        decision: decision.into(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Economics, Governance, Latency, RegistryEntry};

    fn entry(model: &str, input: u64, cached: Option<u64>, output: u64, p50: u64) -> RegistryEntry {
        RegistryEntry {
            endpoint: "openai".into(),
            provider: "openai".into(),
            family: model.into(),
            model: model.into(),
            roles: vec!["solver".into()],
            input_modalities: vec!["text".into()],
            context_tokens: 200_000,
            max_output_tokens: 16_000,
            tools: true,
            vision: false,
            reasoning: false,
            structured_output: true,
            economics: Economics {
                input_per_mtok_minor: input,
                output_per_mtok_minor: output,
                currency: "USD".into(),
                scale: 2,
                cached_input_per_mtok_minor: cached,
                cache_write_per_mtok_minor: Some(input / 4),
                cache_ttl_ms: Some(300_000),
            },
            latency: Latency {
                p50_ms: p50,
                p95_ms: p50 * 2,
            },
            governance: Governance {
                data_residency: "us".into(),
                retains_prompts: false,
                allowed_profiles: vec![],
            },
            revoked: false,
        }
    }

    fn cache(model: &str, tokens: u64, at: i64) -> CacheState {
        CacheState {
            endpoint: "openai".into(),
            model: model.into(),
            cached_prefix_tokens: tokens,
            last_used_at_ms: at,
            prefix_key: "k".into(),
        }
    }

    #[test]
    fn a_nominally_cheaper_model_loses_after_a_cache_miss() {
        // Current: 1000/1M input, cached at 100/1M; alternative: 700/1M, no cache discount.
        let cur = entry("big", 1000, Some(100), 3000, 800);
        let alt = entry("small", 700, None, 2100, 800);
        let demand = RemainingDemand {
            input_tokens_per_call: 40_000,
            output_tokens_per_call: 1_000,
            calls: 5,
        };
        let warm = compare(
            &cur,
            &alt,
            Some(&cache("big", 36_000, 1_000)),
            2_000,
            demand,
            0,
            0,
        );
        assert_eq!(warm.decision, "STAY", "{warm:?}");
        assert!(warm.switch_cost.prefix_warm);
        assert!(warm.switch_cost.re_prefill_minor > 0);
        // The same comparison with the cache expired: the alternative wins.
        let cold = compare(
            &cur,
            &alt,
            Some(&cache("big", 36_000, 1_000)),
            2_000 + 400_000,
            demand,
            0,
            0,
        );
        assert_eq!(cold.decision, "SWITCH", "{cold:?}");
        assert!(!cold.switch_cost.prefix_warm);
        assert_eq!(cold.switch_cost.re_prefill_minor, 0);
        assert!(cold.switch_minor < warm.switch_minor);
        assert!(
            cold.stay_minor > warm.stay_minor,
            "staying is dearer once the prefix is cold"
        );
    }

    #[test]
    fn hysteresis_and_latency_are_explicit_and_can_flip_a_marginal_switch() {
        let cur = entry("a", 1000, Some(1000), 1000, 500);
        let alt = entry("b", 900, Some(900), 1000, 900);
        let demand = RemainingDemand {
            input_tokens_per_call: 100_000,
            output_tokens_per_call: 10_000,
            calls: 10,
        };
        let bare = compare(&cur, &alt, None, 0, demand, 0, 0);
        assert_eq!(bare.decision, "SWITCH", "{bare:?}");
        // Stay 110/call → 1100; switch 100/call → 1000: saves 100. A 10 %
        // margin (110) beats the saving; a 400 ms delay priced at 3 minor
        // units per ms (1200) beats it too.
        let with_hysteresis = compare(&cur, &alt, None, 0, demand, 0, 1_000);
        assert_eq!(with_hysteresis.decision, "STAY", "{with_hysteresis:?}");
        assert_eq!(with_hysteresis.switch_cost.hysteresis_minor, 110);
        let with_latency = compare(&cur, &alt, None, 0, demand, 3, 0);
        assert_eq!(with_latency.decision, "STAY", "{with_latency:?}");
        assert_eq!(with_latency.switch_cost.latency_penalty_minor, 1_200);
    }

    #[test]
    fn nothing_is_counted_twice() {
        let cur = entry("a", 1000, Some(100), 1000, 500);
        let alt = entry("b", 500, Some(50), 500, 500);
        let demand = RemainingDemand {
            input_tokens_per_call: 10_000,
            output_tokens_per_call: 0,
            calls: 1,
        };
        let s = compare(&cur, &alt, Some(&cache("a", 10_000, 0)), 0, demand, 0, 0);
        // Stay: 10k cached at 100/M = 1; switch: 10k at 500/M = 5 + cache write 10k at 125/M = 2 (rounded up).
        assert_eq!(s.stay_minor, 1);
        assert_eq!(s.switch_minor, 5 + 2);
        assert_eq!(s.switch_cost.re_prefill_minor, 5 - 1);
        assert_eq!(s.switch_cost.lost_cache_value_minor, 10 - 1);
        assert_eq!(s.decision, "STAY");
    }
}
