//! Table-driven tests for the BOBO escrow state machine.
//!
//! Every legal transition from the plan spec is exercised, plus a
//! representative sample of illegal transitions to confirm rejection.
//!
//! Legal transitions (from plan):
//!   Held      + Release  → Released
//!   Held      + Dispute  → Disputed
//!   Held      + Refund   → Refunded
//!   Released  + Settle   → Settled
//!   Disputed  + Release  → Released  (merchant-favour resolution)
//!   Disputed  + Refund   → Refunded  (buyer-favour resolution)

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use yaatal_core::commerce::escrow::{transition, EscrowError, EscrowState, EscrowTransition};

struct Case {
    description: &'static str,
    from: EscrowState,
    ev: EscrowTransition,
    expected: Result<EscrowState, EscrowError>,
}

fn cases() -> Vec<Case> {
    vec![
        // ── Legal transitions ─────────────────────────────────────────────
        Case {
            description: "Held + Release → Released",
            from: EscrowState::Held,
            ev: EscrowTransition::Release,
            expected: Ok(EscrowState::Released),
        },
        Case {
            description: "Held + Dispute → Disputed",
            from: EscrowState::Held,
            ev: EscrowTransition::Dispute,
            expected: Ok(EscrowState::Disputed),
        },
        Case {
            description: "Held + Refund → Refunded",
            from: EscrowState::Held,
            ev: EscrowTransition::Refund,
            expected: Ok(EscrowState::Refunded),
        },
        Case {
            description: "Released + Settle → Settled",
            from: EscrowState::Released,
            ev: EscrowTransition::Settle,
            expected: Ok(EscrowState::Settled),
        },
        Case {
            description: "Disputed + Release → Released (merchant-favour)",
            from: EscrowState::Disputed,
            ev: EscrowTransition::Release,
            expected: Ok(EscrowState::Released),
        },
        Case {
            description: "Disputed + Refund → Refunded (buyer-favour)",
            from: EscrowState::Disputed,
            ev: EscrowTransition::Refund,
            expected: Ok(EscrowState::Refunded),
        },
        // ── Illegal transitions ───────────────────────────────────────────
        Case {
            description:
                "Held + Settle → IllegalTransition (cannot settle without releasing first)",
            from: EscrowState::Held,
            ev: EscrowTransition::Settle,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Held,
                transition: EscrowTransition::Settle,
            }),
        },
        Case {
            description: "Released + Release → IllegalTransition (double-release)",
            from: EscrowState::Released,
            ev: EscrowTransition::Release,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Released,
                transition: EscrowTransition::Release,
            }),
        },
        Case {
            description: "Released + Dispute → IllegalTransition (cannot dispute after release)",
            from: EscrowState::Released,
            ev: EscrowTransition::Dispute,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Released,
                transition: EscrowTransition::Dispute,
            }),
        },
        Case {
            description: "Released + Refund → IllegalTransition (cannot refund after release)",
            from: EscrowState::Released,
            ev: EscrowTransition::Refund,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Released,
                transition: EscrowTransition::Refund,
            }),
        },
        Case {
            description: "Settled + Release → IllegalTransition (terminal state)",
            from: EscrowState::Settled,
            ev: EscrowTransition::Release,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Settled,
                transition: EscrowTransition::Release,
            }),
        },
        Case {
            description: "Settled + Dispute → IllegalTransition (terminal state)",
            from: EscrowState::Settled,
            ev: EscrowTransition::Dispute,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Settled,
                transition: EscrowTransition::Dispute,
            }),
        },
        Case {
            description: "Settled + Refund → IllegalTransition (terminal state)",
            from: EscrowState::Settled,
            ev: EscrowTransition::Refund,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Settled,
                transition: EscrowTransition::Refund,
            }),
        },
        Case {
            description: "Settled + Settle → IllegalTransition (double-settle)",
            from: EscrowState::Settled,
            ev: EscrowTransition::Settle,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Settled,
                transition: EscrowTransition::Settle,
            }),
        },
        Case {
            description: "Refunded + Release → IllegalTransition (terminal state)",
            from: EscrowState::Refunded,
            ev: EscrowTransition::Release,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Refunded,
                transition: EscrowTransition::Release,
            }),
        },
        Case {
            description: "Refunded + Dispute → IllegalTransition (terminal state)",
            from: EscrowState::Refunded,
            ev: EscrowTransition::Dispute,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Refunded,
                transition: EscrowTransition::Dispute,
            }),
        },
        Case {
            description: "Disputed + Settle → IllegalTransition (must resolve before settle)",
            from: EscrowState::Disputed,
            ev: EscrowTransition::Settle,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Disputed,
                transition: EscrowTransition::Settle,
            }),
        },
        Case {
            description: "Disputed + Dispute → IllegalTransition (already disputed)",
            from: EscrowState::Disputed,
            ev: EscrowTransition::Dispute,
            expected: Err(EscrowError::IllegalTransition {
                from: EscrowState::Disputed,
                transition: EscrowTransition::Dispute,
            }),
        },
    ]
}

#[test]
fn all_escrow_transitions() {
    let all_cases = cases();
    let total = all_cases.len();
    let mut failures: Vec<String> = Vec::new();

    for case in all_cases {
        let actual = transition(case.from, case.ev);
        if actual != case.expected {
            failures.push(format!(
                "FAIL [{}]: got {:?}, expected {:?}",
                case.description, actual, case.expected,
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} / {} cases failed:\n{}",
            failures.len(),
            total,
            failures.join("\n")
        );
    }

    // All cases passed — print count for the report.
    let legal_count = cases().iter().filter(|c| c.expected.is_ok()).count();
    let illegal_count = total - legal_count;
    let _ = (legal_count, illegal_count); // used in report; suppress lint
}

/// Verify each legal transition individually for clear failure messages.
#[test]
fn legal_held_release() {
    assert_eq!(
        transition(EscrowState::Held, EscrowTransition::Release),
        Ok(EscrowState::Released)
    );
}

#[test]
fn legal_held_dispute() {
    assert_eq!(
        transition(EscrowState::Held, EscrowTransition::Dispute),
        Ok(EscrowState::Disputed)
    );
}

#[test]
fn legal_held_refund() {
    assert_eq!(
        transition(EscrowState::Held, EscrowTransition::Refund),
        Ok(EscrowState::Refunded)
    );
}

#[test]
fn legal_released_settle() {
    assert_eq!(
        transition(EscrowState::Released, EscrowTransition::Settle),
        Ok(EscrowState::Settled)
    );
}

#[test]
fn legal_disputed_release_merchant_favour() {
    assert_eq!(
        transition(EscrowState::Disputed, EscrowTransition::Release),
        Ok(EscrowState::Released)
    );
}

#[test]
fn legal_disputed_refund_buyer_favour() {
    assert_eq!(
        transition(EscrowState::Disputed, EscrowTransition::Refund),
        Ok(EscrowState::Refunded)
    );
}

#[test]
fn illegal_held_settle() {
    assert_eq!(
        transition(EscrowState::Held, EscrowTransition::Settle),
        Err(EscrowError::IllegalTransition {
            from: EscrowState::Held,
            transition: EscrowTransition::Settle,
        })
    );
}

#[test]
fn illegal_settled_is_terminal() {
    for ev in [
        EscrowTransition::Release,
        EscrowTransition::Settle,
        EscrowTransition::Dispute,
        EscrowTransition::Refund,
    ] {
        let result = transition(EscrowState::Settled, ev);
        assert!(
            result.is_err(),
            "Settled + {ev:?} should be illegal, got {result:?}"
        );
    }
}

#[test]
fn illegal_refunded_is_terminal() {
    for ev in [
        EscrowTransition::Release,
        EscrowTransition::Settle,
        EscrowTransition::Dispute,
        EscrowTransition::Refund,
    ] {
        let result = transition(EscrowState::Refunded, ev);
        assert!(
            result.is_err(),
            "Refunded + {ev:?} should be illegal, got {result:?}"
        );
    }
}
