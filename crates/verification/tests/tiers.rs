//! PX-027: a tier is earned by evidence, and a record cannot claim more than
//! its checks show.
use modbit_verification::tiers::{
    CHECKS, CheckOutcome, RejectedRecord, Tier, TierRecord, checks_of, earned_tier, missing_for,
    recorded, tier_of, verify_record,
};

fn outcome(id: &str, status: &str) -> CheckOutcome {
    let check = CHECKS.iter().find(|c| c.id == id).expect("a real check");
    CheckOutcome {
        id: id.into(),
        tier: check.tier,
        status: status.into(),
        detail: format!("{status} in this test"),
    }
}

fn all(status: &str, tiers: &[Tier]) -> Vec<CheckOutcome> {
    tiers
        .iter()
        .flat_map(|t| checks_of(*t).into_iter())
        .map(|c| outcome(c.id, status))
        .collect()
}

fn record(tier: Tier, checks: Vec<CheckOutcome>) -> TierRecord {
    TierRecord {
        language: "fixturelang".into(),
        fixture: "tests/fixtures/repos/none".into(),
        tier,
        checks,
        suite_test: "a_test".into(),
        not_claimed: vec![],
    }
}

#[test]
fn a_tier_is_every_check_of_it_and_of_every_tier_below_it() {
    assert_eq!(earned_tier(&[]), None, "nothing recorded, nothing claimed");
    assert_eq!(earned_tier(&all("pass", &[Tier::C])), Some(Tier::C));
    assert_eq!(
        earned_tier(&all("pass", &[Tier::C, Tier::B])),
        Some(Tier::B)
    );
    assert_eq!(
        earned_tier(&all("pass", &[Tier::C, Tier::B, Tier::A])),
        Some(Tier::A)
    );
    // Tier A checks alone earn nothing: the lower tiers are part of the claim.
    assert_eq!(earned_tier(&all("pass", &[Tier::A])), None);
    // A skipped check is not a pass.
    let mut mixed = all("pass", &[Tier::C, Tier::B]);
    mixed[3] = outcome(checks_of(Tier::B)[0].id, "skip");
    assert_eq!(earned_tier(&mixed), Some(Tier::C));
    assert_eq!(missing_for(Tier::B, &mixed), vec![checks_of(Tier::B)[0].id]);
}

#[test]
fn a_record_that_claims_more_than_it_shows_is_refused() {
    let honest = record(Tier::C, all("pass", &[Tier::C]));
    assert_eq!(verify_record(&honest), Ok(()));
    // Claiming B with only C's checks passing.
    let mut checks = all("pass", &[Tier::C]);
    checks.extend(all("fail", &[Tier::B]));
    let overclaim = record(Tier::B, checks);
    assert_eq!(
        verify_record(&overclaim),
        Err(RejectedRecord::Unearned {
            claimed: Tier::B,
            earned: Some(Tier::C),
            missing: checks_of(Tier::B).iter().map(|c| c.id.to_owned()).collect(),
        })
    );
    // Claiming a tier without carrying its checks at all.
    let silent = record(Tier::B, all("pass", &[Tier::C]));
    assert_eq!(
        verify_record(&silent),
        Err(RejectedRecord::Incomplete {
            absent: checks_of(Tier::B).iter().map(|c| c.id.to_owned()).collect(),
        })
    );
    // An invented check cannot support a claim.
    let mut invented = all("pass", &[Tier::C]);
    invented[0].id = "c0_has_a_grammar".into();
    assert_eq!(
        verify_record(&record(Tier::C, invented)),
        Err(RejectedRecord::UnknownCheck {
            id: "c0_has_a_grammar".into()
        }),
        "a grammar is not a check and a check is not a tier"
    );
}

#[test]
fn every_shipped_record_stands_and_an_unrecorded_language_is_unsupported() {
    for r in recorded() {
        assert_eq!(verify_record(&r), Ok(()), "{r:?}");
        assert!(!r.fixture.is_empty() && !r.suite_test.is_empty(), "{r:?}");
        assert_eq!(tier_of(&r.language), Some(r.tier), "{r:?}");
    }
    assert_eq!(tier_of("go"), None, "a language with no recorded pass");
    assert_eq!(tier_of("cobol"), None);
}
