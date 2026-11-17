#![cfg(not(target_os = "android"))]
//! 文件职责：验证隐藏评级结算、迁移原子性和管理员详情的可见性边界。
use reqwest::{Client, StatusCode};
use rusqlite::{params, Connection};
use serde_json::json;
use serde_json::Value;
use taskrail_lib::server::{self, ServerConfig, ServerMode};
use taskrail_lib::server::{db::Repository, models::*};

struct Kernel {
    repo: Repository,
    project: Project,
    worker: RegisteredAgent,
    _file: tempfile::NamedTempFile,
}

impl Kernel {
    fn new() -> Self {
        let file = tempfile::NamedTempFile::new().unwrap();
        let repo = Repository::open(file.path()).unwrap();
        repo.onboarding(Some(UpdateOnboarding {
            lan_open_registration: true,
        }))
        .unwrap();
        let project = repo
            .create_project(serde_json::from_value(json!({"name":"rating"})).unwrap())
            .unwrap();
        let worker = repo.register_agent(serde_json::from_value(json!({"name":"worker","webhook_url":"http://127.0.0.1:1/callback","webhook_secret":"private-signature"})).unwrap()).unwrap();
        Self {
            repo,
            project,
            worker,
            _file: file,
        }
    }
    fn conn(&self) -> Connection {
        let conn = Connection::open(self.repo.path()).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        conn
    }
    fn detail(&self) -> AgentDetail {
        self.repo.get_agent_detail(&self.worker.agent.id).unwrap()
    }
    fn seed(&self, mmr: f64, settled: i64, as_wins: i64, s_wins: i64) {
        self.conn().execute("UPDATE agents SET mmr=?2,settled_count=?3,verified_as_wins=?4,verified_s_wins=?5,tier='Unranked',placement_completed=0 WHERE id=?1",
            params![self.worker.agent.id,mmr,settled,as_wins,s_wins]).unwrap();
    }
    fn claim(&self, difficulty: Tier, arbitrated: bool, self_dealt: bool) -> Bounty {
        let input = json!({"project_id":self.project.id,"title":"rating task","difficulty":difficulty,"assignee_mode":"any","require_arbitration":arbitrated,"status":"OPEN"});
        let b = if self_dealt {
            let mut input = input;
            input.as_object_mut().unwrap().remove("status");
            self.repo
                .agent_create_bounty(
                    &self.worker.agent.id,
                    serde_json::from_value(input).unwrap(),
                )
                .unwrap()
        } else {
            self.repo
                .create_bounty(serde_json::from_value(input).unwrap())
                .unwrap()
        };
        let b = self
            .repo
            .agent_claim_bounty(&b.id, &self.worker.agent.id)
            .unwrap();
        // 固定已结转耗时，排除机器速度和等待仲裁时间对公式断言的影响。
        self.conn()
            .execute(
                "UPDATE bounties SET active_started_at=NULL WHERE id=?1",
                [&b.id],
            )
            .unwrap();
        b
    }
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &self,
        b: &Bounty,
        grade: Option<QualityGrade>,
        calibrated: Option<Tier>,
        duration: i64,
        cost: Option<f64>,
        rejections: i64,
        judge: Option<&str>,
    ) -> Bounty {
        self.conn()
            .execute(
                "UPDATE bounties SET active_duration_ms=?2,rejection_count=?3 WHERE id=?1",
                params![b.id, duration, rejections],
            )
            .unwrap();
        if b.require_arbitration {
            self.repo
                .agent_submit_arbitration(
                    &b.id,
                    &self.worker.agent.id,
                    SubmitArbitrationInput {
                        lease_id: b.lease_id,
                        completion_note: None,
                        completion_cost: cost,
                    },
                )
                .unwrap();
            self.repo
                .approve_arbitration(
                    &b.id,
                    judge,
                    ApproveArbitrationInput {
                        quality_grade: grade,
                        calibrated_difficulty: calibrated,
                        comment: None,
                    },
                )
                .unwrap()
        } else {
            self.repo
                .agent_complete_bounty(
                    &b.id,
                    &self.worker.agent.id,
                    CompleteBounty {
                        lease_id: b.lease_id,
                        completion_note: None,
                        completion_cost: cost,
                    },
                )
                .unwrap()
        }
    }
    fn fail(&self, b: &Bounty, outcome: RatingOutcome) {
        match outcome {
            RatingOutcome::Abandon => {
                self.repo
                    .agent_abandon_bounty(
                        &b.id,
                        &self.worker.agent.id,
                        AbandonBounty {
                            lease_id: b.lease_id,
                            reason: None,
                        },
                    )
                    .unwrap();
            }
            RatingOutcome::Timeout => {
                self.conn().execute("UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",[&b.id]).unwrap();
                self.repo.sweep_leases().unwrap();
                self.repo.sweep_leases().unwrap();
            }
            RatingOutcome::ArbitrationFailed => {
                for _ in 0..4 {
                    self.repo
                        .agent_submit_arbitration(
                            &b.id,
                            &self.worker.agent.id,
                            SubmitArbitrationInput {
                                lease_id: b.lease_id,
                                completion_note: None,
                                completion_cost: Some(7.0),
                            },
                        )
                        .unwrap();
                    self.repo
                        .reject_arbitration(
                            &b.id,
                            None,
                            RejectArbitrationInput {
                                comment: "fix result".into(),
                            },
                        )
                        .unwrap();
                }
            }
            RatingOutcome::Completed => panic!("expected a failure outcome"),
        }
    }
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn fifth_result_assigns_tier_immediately_and_sixth_uses_regular_k_with_persisted_history() {
    let k = Kernel::new();
    for n in 1..=6 {
        let b = k.claim(Tier::C, false, false);
        let completed = k.finish(&b, None, None, 0, None, 0, None);
        let d = k.detail();
        assert_eq!(d.placement_count, n.min(5));
        assert_eq!(d.agent.placement_completed, n >= 5);
        assert_eq!(
            d.agent.tier,
            if n < 5 {
                AgentTier::Unranked
            } else {
                AgentTier::C
            }
        );
        assert_eq!(completed.lease_agent_tier, Some(d.agent.tier));
        assert_eq!(d.completed_count, n);
        assert_eq!(d.failed_count, 0);
        assert_eq!(d.ledger.len(), n as usize);
        let e = &d.ledger[0];
        assert_eq!(e.is_placement, n <= 5);
        near(e.k_factor, if n <= 5 { 80.0 } else { 32.0 });
        assert_eq!(e.bounty_id, b.id);
        assert_eq!(e.bounty_title, b.title);
        assert_eq!(e.project_id, b.project_id);
        assert_eq!(e.lease_id, b.lease_id);
        near(e.mmr_after, d.mmr);
        if n > 1 {
            assert!(e.id > d.ledger[1].id);
            near(e.mmr_before, d.ledger[1].mmr_after);
        }
        let reopened = Repository::open(k.repo.path())
            .unwrap()
            .get_agent_detail(&k.worker.agent.id)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&d).unwrap(),
            serde_json::to_value(reopened).unwrap()
        );
        assert!(k
            .repo
            .agent_complete_bounty(
                &b.id,
                &k.worker.agent.id,
                CompleteBounty {
                    lease_id: b.lease_id,
                    completion_note: None,
                    completion_cost: None
                }
            )
            .is_err());
        assert_eq!(k.detail().ledger.len(), n as usize);
    }
}

#[test]
fn formula_uses_quality_rejection_decay_and_each_difficulty_benchmark() {
    let k = Kernel::new();
    for (difficulty, target, duration, cost) in [
        (Tier::C, 1000.0, 600_000, 1.0),
        (Tier::B, 1350.0, 1_200_000, 3.0),
        (Tier::A, 1650.0, 1_800_000, 8.0),
        (Tier::S, 1950.0, 2_700_000, 15.0),
    ] {
        for (arbitrated, grade, q_base) in [
            (true, Some(QualityGrade::Perfect), 1.0),
            (true, Some(QualityGrade::Flawed), 0.85),
            (false, None, 0.9),
        ] {
            for rejections in 0..=3 {
                k.seed(1000.0, 0, 0, 0);
                let b = k.claim(difficulty, arbitrated, false);
                k.finish(
                    &b,
                    grade,
                    None,
                    duration,
                    Some(cost * 2.0),
                    rejections,
                    None,
                );
                let e = k.detail().ledger.remove(0);
                near(e.quality_score, q_base * 0.85_f64.powi(rejections as i32));
                near(e.time_efficiency, 0.5);
                near(e.cost_efficiency, 1.0 / 3.0);
                near(
                    e.expected_score,
                    1.0 / (1.0 + 10.0_f64.powf((target - 1000.0) / 400.0)),
                );
                near(
                    e.actual_score,
                    e.quality_score * (0.75 + 0.15 * 0.5 + 0.1 / 3.0),
                );
                near(e.raw_delta, 80.0 * (e.actual_score - e.expected_score));
                near(e.applied_delta, e.raw_delta);
                assert_eq!(e.rejection_count, rejections);
                assert_eq!(e.active_duration_ms, duration);
                assert_eq!(e.completion_cost, Some(cost * 2.0));
                assert_eq!(e.quality_grade, grade);
            }
        }
    }
    for (duration, cost, time_eff, cost_eff) in [
        (0, None, 1.0, 0.5),
        (0, Some(0.0), 1.0, 1.0),
        (1_800_000, Some(1.0), 0.25, 0.5),
    ] {
        k.seed(1000.0, 0, 0, 0);
        let b = k.claim(Tier::C, false, false);
        k.finish(&b, None, None, duration, cost, 0, None);
        let e = k.detail().ledger.remove(0);
        near(e.time_efficiency, time_eff);
        near(e.cost_efficiency, cost_eff);
    }
}

#[test]
fn difficulty_caps_clamp_positive_gain_without_waiving_failure_deductions() {
    let k = Kernel::new();
    for (difficulty, cap) in [(Tier::C, 1199.0), (Tier::B, 1499.0), (Tier::A, 1799.0)] {
        for before in [cap - 1.0, cap, cap + 100.0] {
            k.seed(before, 5, 3, 0);
            let b = k.claim(difficulty, true, false);
            k.finish(&b, Some(QualityGrade::Perfect), None, 0, Some(0.0), 0, None);
            let e = k.detail().ledger.remove(0);
            assert!(e.raw_delta > 0.0);
            near(e.mmr_after, before.max(cap));
            near(e.applied_delta, if before < cap { 1.0 } else { 0.0 });
            assert_eq!(e.cap_reason.as_deref(), Some("difficulty_cap"));
        }
        k.seed(cap + 100.0, 5, 3, 0);
        let b = k.claim(difficulty, true, false);
        k.fail(&b, RatingOutcome::Abandon);
        let e = k.detail().ledger.remove(0);
        near(e.raw_delta, -32.0 * e.expected_score);
        near(e.applied_delta, e.raw_delta);
        assert!(e.cap_reason.is_none());
    }
}

#[test]
fn unarbitrated_cap_uses_tightest_ceiling_and_regular_negative_completion_is_not_waived() {
    let k = Kernel::new();
    for difficulty in [Tier::B, Tier::A, Tier::S] {
        for before in [1349.0, 1350.0, 1400.0] {
            k.seed(before, 5, 0, 0);
            let b = k.claim(difficulty, false, false);
            k.finish(&b, None, None, 0, Some(0.0), 0, None);
            let e = k.detail().ledger.remove(0);
            assert!(e.raw_delta > 0.0);
            near(e.mmr_after, before.max(1350.0));
            assert_eq!(e.cap_reason.as_deref(), Some("unarbitrated_cap"));
        }
    }
    k.seed(1200.0, 5, 0, 0);
    let b = k.claim(Tier::C, false, false);
    k.finish(&b, None, None, 0, Some(0.0), 0, None);
    assert_eq!(
        k.detail().ledger[0].cap_reason.as_deref(),
        Some("difficulty_cap")
    );
    k.seed(2200.0, 5, 3, 5);
    let b = k.claim(Tier::C, false, true);
    k.finish(&b, None, None, 3_600_000, Some(100.0), 3, None);
    let e = k.detail().ledger.remove(0);
    assert!(e.raw_delta < 0.0);
    near(e.applied_delta, e.raw_delta);
    assert!(e.cap_reason.is_none());
}

#[test]
fn self_deal_blocks_positive_gain_but_failures_and_placement_still_count() {
    let k = Kernel::new();
    for arbitrated in [false, true] {
        k.seed(1000.0, 0, 0, 0);
        let b = k.claim(Tier::S, arbitrated, true);
        k.finish(
            &b,
            arbitrated.then_some(QualityGrade::Perfect),
            None,
            0,
            None,
            0,
            None,
        );
        let d = k.detail();
        let e = &d.ledger[0];
        assert!(e.self_dealt);
        assert!(e.raw_delta > 0.0);
        near(e.applied_delta, 0.0);
        near(d.mmr, 1000.0);
        assert_eq!(e.cap_reason.as_deref(), Some("self_deal"));
        assert_eq!(d.placement_count, 1);
        let b = k.claim(Tier::S, arbitrated, true);
        k.fail(&b, RatingOutcome::Abandon);
        let e = k.detail().ledger.remove(0);
        assert!(e.self_dealt);
        assert!(e.applied_delta < 0.0);
        assert!(e.cap_reason.is_none());
    }
}

#[test]
fn calibrated_difficulty_controls_target_cap_and_verified_win_credit() {
    let k = Kernel::new();
    for (original, effective, before, target, cap, as_credit, s_credit) in [
        (Tier::S, Tier::C, 1198.0, 1000.0, Some(1199.0), 0, 0),
        (Tier::S, Tier::B, 1498.0, 1350.0, Some(1499.0), 0, 0),
        (Tier::C, Tier::A, 1798.0, 1650.0, Some(1799.0), 1, 0),
        (Tier::C, Tier::S, 1900.0, 1950.0, None, 1, 1),
    ] {
        k.seed(before, 5, 0, 0);
        let b = k.claim(original, true, false);
        k.finish(
            &b,
            Some(QualityGrade::Perfect),
            Some(effective),
            0,
            Some(0.0),
            0,
            None,
        );
        let d = k.detail();
        let e = &d.ledger[0];
        assert_eq!(e.original_difficulty, original);
        assert_eq!(e.effective_difficulty, effective);
        near(
            e.expected_score,
            1.0 / (1.0 + 10.0_f64.powf((target - before) / 400.0)),
        );
        if let Some(cap) = cap {
            near(e.mmr_after, cap);
            assert_eq!(e.cap_reason.as_deref(), Some("difficulty_cap"));
        } else {
            assert!(e.mmr_after > before);
            assert!(e.cap_reason.is_none());
        }
        assert_eq!(d.verified_as_wins, as_credit);
        assert_eq!(d.verified_s_wins, s_credit);
    }
}

#[test]
fn human_and_agent_verified_wins_unlock_a_and_s_on_exact_required_win() {
    let k = Kernel::new();
    let judge = k
        .repo
        .register_agent(serde_json::from_value(json!({"name":"judge"})).unwrap())
        .unwrap();
    k.repo
        .arbitration_config(Some(ArbitrationConfig {
            arbitrator_chain: vec![judge.agent.id.clone()],
        }))
        .unwrap();
    k.seed(1600.0, 5, 0, 0);
    for win in 1..=3 {
        let b = k.claim(Tier::A, true, false);
        let updated = k.finish(
            &b,
            Some(QualityGrade::Perfect),
            None,
            0,
            Some(0.0),
            0,
            if win % 2 == 0 {
                Some(&judge.agent.id)
            } else {
                None
            },
        );
        let d = k.detail();
        assert_eq!(d.verified_as_wins, win);
        assert_eq!(
            d.agent.tier,
            if win < 3 { AgentTier::B } else { AgentTier::A }
        );
        assert_eq!(updated.lease_agent_tier, Some(d.agent.tier));
        assert_eq!(
            d.ledger[0].tier_gate_reason.as_deref(),
            if win < 3 { Some("needs_as_wins") } else { None }
        );
    }
    k.seed(1900.0, 5, 3, 0);
    for win in 1..=5 {
        let b = k.claim(Tier::S, true, false);
        let updated = k.finish(
            &b,
            Some(QualityGrade::Perfect),
            None,
            0,
            Some(0.0),
            0,
            if win % 2 == 0 {
                Some(&judge.agent.id)
            } else {
                None
            },
        );
        let d = k.detail();
        assert_eq!(d.verified_s_wins, win);
        assert_eq!(
            d.agent.tier,
            if win < 5 { AgentTier::A } else { AgentTier::S }
        );
        assert_eq!(updated.lease_agent_tier, Some(d.agent.tier));
        assert_eq!(
            d.ledger[0].tier_gate_reason.as_deref(),
            if win < 5 { Some("needs_s_wins") } else { None }
        );
    }
    k.seed(1900.0, 5, 0, 0);
    let b = k.claim(Tier::S, true, false);
    k.finish(&b, Some(QualityGrade::Perfect), None, 0, Some(0.0), 0, None);
    assert_eq!(k.detail().agent.tier, AgentTier::B);
    assert_eq!(
        k.detail().ledger[0].tier_gate_reason.as_deref(),
        Some("needs_as_wins")
    );
}

#[test]
fn timeout_abandon_and_fourth_rejection_settle_once_and_keep_failure_evidence() {
    let k = Kernel::new();
    for (n, outcome) in [
        RatingOutcome::Timeout,
        RatingOutcome::Abandon,
        RatingOutcome::ArbitrationFailed,
    ]
    .into_iter()
    .enumerate()
    {
        let b = k.claim(Tier::A, true, true);
        k.conn()
            .execute(
                "UPDATE bounties SET active_duration_ms=1234 WHERE id=?1",
                [&b.id],
            )
            .unwrap();
        k.fail(&b, outcome);
        let d = k.detail();
        let e = &d.ledger[0];
        assert_eq!(e.outcome, outcome);
        assert!(e.arbitrated);
        assert!(e.self_dealt);
        near(e.quality_score, 0.0);
        near(e.time_efficiency, 0.0);
        near(e.cost_efficiency, 0.0);
        near(e.actual_score, 0.0);
        near(e.raw_delta, -80.0 * e.expected_score);
        near(e.applied_delta, e.raw_delta);
        assert!(e.applied_delta < 0.0);
        assert!(e.active_duration_ms >= 1234);
        assert_eq!(
            e.rejection_count,
            if outcome == RatingOutcome::ArbitrationFailed {
                4
            } else {
                0
            }
        );
        assert_eq!(
            e.completion_cost,
            if outcome == RatingOutcome::ArbitrationFailed {
                Some(7.0)
            } else {
                None
            }
        );
        assert!(e.cap_reason.is_none());
        assert_eq!(d.placement_count, n as i64 + 1);
        assert_eq!(d.failed_count, n as i64 + 1);
        assert_eq!(d.completed_count, 0);
        assert_eq!(d.verified_as_wins, 0);
        assert_eq!(d.ledger.len(), n + 1);
        assert_eq!(
            k.repo.get_bounty_detail(&b.id).unwrap().bounty.status,
            BountyStatus::Open
        );
    }
    let d = k.detail();
    assert_eq!(d.agent.timeout_count, 1);
    assert_eq!(d.agent.abandon_count, 1);
}

#[test]
fn rejected_then_abandon_and_timeout_keep_declared_cost_in_ledger_and_clear_bounty_on_reopen() {
    let k = Kernel::new();
    for outcome in [RatingOutcome::Abandon, RatingOutcome::Timeout] {
        let b = k.claim(Tier::A, true, false);
        k.repo
            .agent_submit_arbitration(
                &b.id,
                &k.worker.agent.id,
                SubmitArbitrationInput {
                    lease_id: b.lease_id,
                    completion_note: Some("partial delivery".into()),
                    completion_cost: Some(7.0),
                },
            )
            .unwrap();
        let rejected = k
            .repo
            .reject_arbitration(
                &b.id,
                None,
                RejectArbitrationInput {
                    comment: "need revision".into(),
                },
            )
            .unwrap();
        assert_eq!(rejected.status, BountyStatus::InProgress);
        assert_eq!(rejected.rejection_count, 1);
        assert_eq!(rejected.completion_cost, Some(7.0));

        k.fail(&b, outcome);

        let d = k.detail();
        let e = &d.ledger[0];
        assert_eq!(e.outcome, outcome);
        assert_eq!(e.rejection_count, 1);
        assert_eq!(e.completion_cost, Some(7.0));
        near(e.cost_efficiency, 0.0);
        assert!(e.applied_delta < 0.0);

        let reopened = k.repo.get_bounty_detail(&b.id).unwrap().bounty;
        assert_eq!(reopened.status, BountyStatus::Open);
        assert_eq!(reopened.rejection_count, 0);
        assert_eq!(reopened.completion_cost, None);
        assert_eq!(reopened.completion_note, None);
    }
}

#[test]
fn admin_release_never_rates_or_records_incidents_and_legacy_leases_skip_rating() {
    let k = Kernel::new();
    for state in ["IN_PROGRESS", "WAITING_INFO", "PENDING_ARBITRATION"] {
        let b = k.claim(Tier::S, true, false);
        k.conn()
            .execute(
                "UPDATE bounties SET status=?2 WHERE id=?1",
                params![b.id, state],
            )
            .unwrap();
        let before = serde_json::to_value(k.detail()).unwrap();
        k.repo.admin_release_lease(&b.id).unwrap();
        let after = k.detail();
        near(after.mmr, 1000.0);
        assert!(after.ledger.is_empty());
        assert_eq!(before["placement_count"], json!(after.placement_count));
        assert_eq!(after.failed_count, 0);
        assert_eq!(after.agent.timeout_count, 0);
        assert_eq!(after.agent.abandon_count, 0);
        assert_eq!(
            k.conn()
                .query_row("SELECT COUNT(*) FROM lease_incidents", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    for owner in [Some("legacy-agent"), None] {
        let b = k.claim(Tier::C, false, false);
        k.conn().execute("UPDATE bounties SET lease_agent_id=?2,lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",params![b.id,owner]).unwrap();
        k.repo.sweep_leases().unwrap();
        assert_eq!(
            k.repo.get_bounty_detail(&b.id).unwrap().bounty.status,
            BountyStatus::Open
        );
        assert!(k.detail().ledger.is_empty());
    }
}

fn assert_public(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (name, value) in fields {
                assert!(
                    ![
                        "mmr",
                        "raw_delta",
                        "applied_delta",
                        "mmr_before",
                        "mmr_after",
                        "api_key",
                        "api_key_hash",
                        "webhook_secret"
                    ]
                    .contains(&name.as_str()),
                    "private field {name}"
                );
                assert_public(value);
            }
        }
        Value::Array(values) => values.iter().for_each(assert_public),
        _ => {}
    }
}

fn assert_json_near(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => near(a.as_f64().unwrap(), b.as_f64().unwrap()),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_json_near(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in a {
                assert_json_near(value, &b[key]);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

#[tokio::test]
async fn http_detail_requires_admin_while_all_public_dtos_and_events_hide_rating_and_secrets() {
    let k = Kernel::new();
    let b = k.claim(Tier::S, true, false);
    k.finish(
        &b,
        Some(QualityGrade::Perfect),
        Some(Tier::A),
        1234,
        Some(2.5),
        1,
        None,
    );
    let failed = k.claim(Tier::C, false, false);
    k.fail(&failed, RatingOutcome::Abandon);
    let server = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: k.repo.path().to_owned(),
        admin_token: "rating-admin".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let base = &server.info.base_url;
    let client = Client::new();
    for token in [None, Some(k.worker.api_key.as_str())] {
        let request = client.get(format!("{base}/api/v1/agents/{}", k.worker.agent.id));
        let request = match token {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        assert_eq!(
            request.send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    for path in [
        format!("/api/v1/agents/{}", k.worker.agent.id),
        "/api/v1/agents/unknown".into(),
        "/api/v1/agents".into(),
    ] {
        for token in [None, Some(k.worker.api_key.as_str())] {
            let mut request = client.get(format!("{base}{path}"));
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            assert_eq!(
                request.send().await.unwrap().status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }
    let missing = client
        .get(format!("{base}/api/v1/agents/unknown"))
        .bearer_auth("rating-admin")
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing.json::<Value>().await.unwrap()["error"]["code"],
        "AGENT_NOT_FOUND"
    );
    let response = client
        .get(format!("{base}/api/v1/agents/{}", k.worker.agent.id))
        .bearer_auth("rating-admin")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let detail: AgentDetail = response.json().await.unwrap();
    let expected = k.detail();
    near(detail.mmr, expected.mmr);
    assert_eq!(detail.placement_count, 2);
    assert_eq!(detail.placement_required, 5);
    assert_eq!(detail.completed_count, 1);
    assert_eq!(detail.failed_count, 1);
    assert_eq!(detail.verified_as_wins, 1);
    assert_eq!(detail.verified_s_wins, 0);
    assert_json_near(
        &serde_json::to_value(&detail.ledger).unwrap(),
        &serde_json::to_value(&expected.ledger).unwrap(),
    );
    assert_public(&serde_json::to_value(&detail.agent).unwrap());
    let private_json = serde_json::to_value(&detail).unwrap();
    for field in ["api_key", "api_key_hash", "webhook_secret"] {
        assert!(!private_json.to_string().contains(&format!("\"{field}\"")));
    }
    for (path, token) in [
        ("/api/v1/agents".to_owned(), "rating-admin"),
        ("/api/v1/agent/me".to_owned(), k.worker.api_key.as_str()),
        (
            "/api/v1/agent/bounties".to_owned(),
            k.worker.api_key.as_str(),
        ),
        (
            format!("/api/v1/agent/bounties/{}", b.id),
            k.worker.api_key.as_str(),
        ),
        (
            format!("/api/v1/agent/bounties/{}/comments", b.id),
            k.worker.api_key.as_str(),
        ),
        (
            "/api/v1/agent/events?cursor=0".to_owned(),
            k.worker.api_key.as_str(),
        ),
        ("/api/v1/bounties".to_owned(), "rating-admin"),
        (format!("/api/v1/bounties/{}", b.id), "rating-admin"),
    ] {
        let response = client
            .get(format!("{base}{path}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_public(&response.json::<Value>().await.unwrap());
    }
    for agent in k.repo.list_agents(AgentFilter::default()).unwrap() {
        assert_public(&serde_json::to_value(agent).unwrap());
    }
    assert_public(&serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap());
    assert_public(
        &serde_json::to_value(
            k.repo
                .agent_events(&k.worker.agent.id, Some(0), 500)
                .unwrap(),
        )
        .unwrap(),
    );
    let skill = client
        .get(format!("{base}/skill.md"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for text in [
        "前 5 单",
        "Unranked / C / B / A / S",
        "打回次数",
        "执行耗时",
        "申报成本",
        "自发自接",
        "A/S 胜场",
        "S 胜场",
        "仅供管理员查看",
    ] {
        assert!(skill.contains(text), "{text}");
    }
    assert!(!skill.contains(&k.worker.api_key));
    assert!(!skill.contains("raw_delta"));
    assert!(!skill.contains(&format!("mmr: {}", detail.mmr)));
    server.shutdown().await.unwrap();
}

fn legacy_v7(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(include_str!("fixtures/server_v1.sql"))
        .unwrap();
    for schema in [
        include_str!("../src/server/schema_v2.sql"),
        include_str!("../src/server/schema_v3.sql"),
        include_str!("../src/server/schema_v4.sql"),
        include_str!("../src/server/schema_v5.sql"),
        include_str!("../src/server/schema_v6.sql"),
        include_str!("../src/server/schema_v7.sql"),
    ] {
        conn.execute_batch(schema).unwrap();
    }
    conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL); INSERT INTO schema_migrations VALUES(7,'old'); INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','legacy','old','old'); INSERT INTO agents(id,name,api_key_hash,api_key_prefix,mmr,tier,created_at,updated_at) VALUES('a','legacy','hash','prefix',1612.5,'A','old','old'); INSERT INTO bounties(id,project_id,title,difficulty,assignee_mode,status,lease_id,lease_agent_id,created_at,updated_at) VALUES('b','p','legacy task','A','any','IN_PROGRESS',42,'a','old','old'); INSERT INTO bounty_events(bounty_id,event_type,actor,to_status,created_at) VALUES('b','created','human','IN_PROGRESS','old');").unwrap();
    conn
}

fn schema(conn: &Connection) -> Vec<(String, Option<String>)> {
    let mut stmt = conn
        .prepare("SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn v7_to_v8_is_atomic_idempotent_and_preserves_existing_rating_and_tasks() {
    for failure in ["none", "table", "index", "version"] {
        let file = tempfile::NamedTempFile::new().unwrap();
        let conn = legacy_v7(file.path());
        match failure {
            "table" => conn.execute_batch("CREATE TABLE agent_rating_ledger(conflict TEXT);").unwrap(),
            "index" => conn.execute_batch("CREATE TABLE agent_rating_ledger_agent_order(conflict TEXT);").unwrap(),
            "version" => conn.execute_batch("CREATE TRIGGER fail_version BEFORE INSERT ON schema_migrations WHEN NEW.version=8 BEGIN SELECT RAISE(ABORT,'version'); END;").unwrap(),
            _ => {}
        }
        let before = schema(&conn);
        if failure != "none" {
            assert!(Repository::open(file.path()).is_err());
            assert_eq!(schema(&conn), before);
            assert_eq!(
                conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                7
            );
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name IN ('settled_count','completed_count','failed_count','verified_as_wins','verified_s_wins')",[],|r|r.get::<_,i64>(0)).unwrap(),0);
            conn.execute_batch(match failure {
                "table" => "DROP TABLE agent_rating_ledger;",
                "index" => "DROP TABLE agent_rating_ledger_agent_order;",
                _ => "DROP TRIGGER fail_version;",
            })
            .unwrap();
        }
        let repo = Repository::open(file.path()).unwrap();
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            9
        );
        let d = repo.get_agent_detail("a").unwrap();
        near(d.mmr, 1612.5);
        assert_eq!(d.agent.tier, AgentTier::A);
        assert_eq!(d.placement_count, 0);
        assert_eq!(d.completed_count, 0);
        assert_eq!(d.failed_count, 0);
        assert_eq!(d.verified_as_wins, 0);
        assert_eq!(d.verified_s_wins, 0);
        assert!(d.ledger.is_empty());
        let task = repo.get_bounty_detail("b").unwrap();
        assert_eq!(task.bounty.title, "legacy task");
        assert_eq!(task.bounty.lease_id, 42);
        assert_eq!(task.events.len(), 1);
        let after = schema(&conn);
        let reopened = Repository::open(file.path()).unwrap();
        assert_eq!(schema(&conn), after);
        near(reopened.get_agent_detail("a").unwrap().mmr, 1612.5);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version=8",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM pragma_index_info('agent_rating_ledger_agent_order') WHERE name IN ('agent_id','id')",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    }
}

fn snapshot(conn: &Connection) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    [
        "agents",
        "bounties",
        "agent_rating_ledger",
        "bounty_events",
        "agent_events",
        "bounty_comments",
        "lease_incidents",
        "bounty_dispatches",
    ]
    .into_iter()
    .map(|table| {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = stmt.column_count();
        stmt.query_map([], |r| {
            (0..columns)
                .map(|i| r.get(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    })
    .collect()
}

#[test]
fn settlement_and_all_state_events_incidents_and_counters_roll_back_together() {
    for outcome in [
        RatingOutcome::Completed,
        RatingOutcome::Abandon,
        RatingOutcome::Timeout,
        RatingOutcome::ArbitrationFailed,
    ] {
        for failure in ["ledger", "stats", "history"] {
            for arbitrated in if outcome == RatingOutcome::Completed {
                vec![false, true]
            } else {
                vec![true]
            } {
                let k = Kernel::new();
                let b = k.claim(Tier::A, arbitrated, false);
                if outcome == RatingOutcome::ArbitrationFailed {
                    for _ in 0..3 {
                        k.repo
                            .agent_submit_arbitration(
                                &b.id,
                                &k.worker.agent.id,
                                SubmitArbitrationInput {
                                    lease_id: b.lease_id,
                                    completion_note: None,
                                    completion_cost: None,
                                },
                            )
                            .unwrap();
                        k.repo
                            .reject_arbitration(
                                &b.id,
                                None,
                                RejectArbitrationInput {
                                    comment: "retry".into(),
                                },
                            )
                            .unwrap();
                    }
                }
                if arbitrated
                    && matches!(
                        outcome,
                        RatingOutcome::Completed | RatingOutcome::ArbitrationFailed
                    )
                {
                    k.repo
                        .agent_submit_arbitration(
                            &b.id,
                            &k.worker.agent.id,
                            SubmitArbitrationInput {
                                lease_id: b.lease_id,
                                completion_note: None,
                                completion_cost: Some(2.0),
                            },
                        )
                        .unwrap();
                }
                let conn = k.conn();
                if outcome == RatingOutcome::Timeout {
                    conn.execute("UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",[&b.id]).unwrap();
                }
                conn.execute_batch(match failure {
                    "ledger" => "CREATE TRIGGER fail BEFORE INSERT ON agent_rating_ledger BEGIN SELECT RAISE(ABORT,'ledger'); END;",
                    "stats" => "CREATE TRIGGER fail BEFORE UPDATE OF mmr ON agents BEGIN SELECT RAISE(ABORT,'stats'); END;",
                    _ => "CREATE TRIGGER fail BEFORE INSERT ON bounty_events BEGIN SELECT RAISE(ABORT,'history'); END;",
                }).unwrap();
                let before = snapshot(&conn);
                let result = match outcome {
                    RatingOutcome::Completed if arbitrated => k
                        .repo
                        .approve_arbitration(
                            &b.id,
                            None,
                            ApproveArbitrationInput {
                                quality_grade: Some(QualityGrade::Perfect),
                                calibrated_difficulty: Some(Tier::S),
                                comment: Some("approved".into()),
                            },
                        )
                        .map(|_| ()),
                    RatingOutcome::Completed => k
                        .repo
                        .agent_complete_bounty(
                            &b.id,
                            &k.worker.agent.id,
                            CompleteBounty {
                                lease_id: b.lease_id,
                                completion_note: None,
                                completion_cost: None,
                            },
                        )
                        .map(|_| ()),
                    RatingOutcome::Abandon => k
                        .repo
                        .agent_abandon_bounty(
                            &b.id,
                            &k.worker.agent.id,
                            AbandonBounty {
                                lease_id: b.lease_id,
                                reason: None,
                            },
                        )
                        .map(|_| ()),
                    RatingOutcome::Timeout => k.repo.sweep_leases(),
                    RatingOutcome::ArbitrationFailed => k
                        .repo
                        .reject_arbitration(
                            &b.id,
                            None,
                            RejectArbitrationInput {
                                comment: "failed".into(),
                            },
                        )
                        .map(|_| ()),
                };
                assert!(result.is_err(), "{outcome:?} {failure}");
                assert_eq!(snapshot(&conn), before, "{outcome:?} {failure}");
            }
        }
    }
}

#[test]
fn new_agent_has_hidden_initial_rating_and_no_placement_results() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let repo = Repository::open(file.path()).unwrap();
    repo.onboarding(Some(UpdateOnboarding {
        lan_open_registration: true,
    }))
    .unwrap();
    let agent = repo
        .register_agent(serde_json::from_value(json!({"name":"worker"})).unwrap())
        .unwrap();
    let detail = repo.get_agent_detail(&agent.agent.id).unwrap();
    assert_eq!(detail.mmr, 1000.0);
    assert_eq!(detail.agent.tier, AgentTier::Unranked);
    assert!(!detail.agent.placement_completed);
    assert_eq!(detail.placement_count, 0);
    assert_eq!(detail.placement_required, 5);
    assert!(detail.ledger.is_empty());
}

#[test]
fn concurrent_completions_settle_once_and_reclaimed_leases_have_distinct_entries() {
    let k = Kernel::new();
    let b = k.claim(Tier::C, false, false);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            let repo = Repository::open(k.repo.path()).unwrap();
            let bounty_id = b.id.clone();
            let agent_id = k.worker.agent.id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                repo.agent_complete_bounty(
                    &bounty_id,
                    &agent_id,
                    CompleteBounty {
                        lease_id: 1,
                        completion_note: None,
                        completion_cost: None,
                    },
                )
            })
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(Result::is_ok)
            .count(),
        1
    );
    assert_eq!(k.detail().ledger.len(), 1);
    assert_eq!(k.detail().completed_count, 1);
    k.seed(1200.0, 5, 0, 0);
    let b = k.claim(Tier::C, false, false);
    k.fail(&b, RatingOutcome::Abandon);
    let d = k.detail();
    assert_eq!(d.agent.tier, AgentTier::C);
    let reclaimed = k
        .repo
        .agent_claim_bounty(&b.id, &k.worker.agent.id)
        .unwrap();
    assert_eq!(reclaimed.lease_id, 2);
    k.finish(&reclaimed, None, None, 0, None, 0, None);
    let d = k.detail();
    assert_eq!(d.ledger[0].bounty_id, b.id);
    assert_eq!(d.ledger[1].bounty_id, b.id);
    assert_eq!(d.ledger[0].lease_id, 2);
    assert_eq!(d.ledger[1].lease_id, 1);
    assert_eq!(d.ledger[1].outcome, RatingOutcome::Abandon);
    near(d.ledger[0].mmr_before, d.ledger[1].mmr_after);
}

#[test]
fn rating_ledger_enforces_enum_and_foreign_key_constraints_and_cascades_deletions() {
    let k = Kernel::new();
    let b = k.claim(Tier::A, true, false);
    k.finish(&b, Some(QualityGrade::Perfect), None, 0, None, 0, None);
    let conn = k.conn();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    for sql in [
        "UPDATE agent_rating_ledger SET outcome='unknown'",
        "UPDATE agent_rating_ledger SET original_difficulty='D'",
        "UPDATE agent_rating_ledger SET effective_difficulty='Unranked'",
        "UPDATE agent_rating_ledger SET quality_grade='average'",
        "UPDATE agent_rating_ledger SET tier_before='D'",
        "UPDATE agent_rating_ledger SET tier_after='D'",
        "UPDATE agent_rating_ledger SET agent_id='missing'",
        "UPDATE agent_rating_ledger SET bounty_id='missing'",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(k.detail().ledger.len(), 1);
    conn.execute("DELETE FROM bounties WHERE id=?1", [&b.id])
        .unwrap();
    assert!(k.detail().ledger.is_empty());
    let b = k.claim(Tier::C, false, false);
    k.finish(&b, None, None, 0, None, 0, None);
    conn.execute("DELETE FROM agents WHERE id=?1", [&k.worker.agent.id])
        .unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM agent_rating_ledger", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        k.repo
            .get_agent_detail(&k.worker.agent.id)
            .unwrap_err()
            .code,
        "AGENT_NOT_FOUND"
    );
}
