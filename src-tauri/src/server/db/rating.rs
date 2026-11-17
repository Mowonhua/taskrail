//! 文件职责：纯评级计算与事务内结算；隐藏分只写入 agents 和管理员专用流水。
use super::*;

struct RatingState {
    mmr: f64,
    tier: AgentTier,
    settled_count: i64,
    verified_as_wins: i64,
    verified_s_wins: i64,
}

fn benchmark(difficulty: Tier) -> (f64, f64, f64, Option<f64>) {
    match difficulty {
        Tier::C => (1000.0, 600_000.0, 1.0, Some(1199.0)),
        Tier::B => (1350.0, 1_200_000.0, 3.0, Some(1499.0)),
        Tier::A => (1650.0, 1_800_000.0, 8.0, Some(1799.0)),
        Tier::S => (1950.0, 2_700_000.0, 15.0, None),
    }
}

fn assigned_tier(mmr: f64, settled: i64, as_wins: i64, s_wins: i64) -> AgentTier {
    if settled < 5 {
        return AgentTier::Unranked;
    }
    if mmr >= 1800.0 && s_wins >= 5 && as_wins >= 3 {
        return AgentTier::S;
    }
    if mmr >= 1500.0 && as_wins >= 3 {
        return AgentTier::A;
    }
    if mmr >= 1200.0 {
        return AgentTier::B;
    }
    AgentTier::C
}

fn calculate(
    bounty: &Bounty,
    agent_id: &str,
    state: &RatingState,
    outcome: RatingOutcome,
    arbitrated: bool,
    timestamp: &str,
) -> AgentRatingLedgerEntry {
    let completed = outcome == RatingOutcome::Completed;
    let effective_difficulty = if arbitrated {
        bounty.calibrated_difficulty.unwrap_or(bounty.difficulty)
    } else {
        bounty.difficulty
    };
    let (target, duration, cost, diff_cap) = benchmark(effective_difficulty);
    let is_placement = state.settled_count < 5;
    let k_factor = if is_placement { 80.0 } else { 32.0 };
    let expected_score = 1.0 / (1.0 + 10.0_f64.powf((target - state.mmr) / 400.0));
    let (quality_score, time_efficiency, cost_efficiency) = if completed {
        let q_base = match (arbitrated, bounty.quality_grade) {
            (true, Some(QualityGrade::Perfect)) => 1.0,
            (true, Some(QualityGrade::Flawed)) => 0.85,
            _ => 0.9,
        };
        (
            q_base * 0.85_f64.powi(bounty.rejection_count.max(0) as i32),
            1.0 / (1.0 + bounty.active_duration_ms.max(0) as f64 / duration),
            bounty
                .completion_cost
                .map_or(0.5, |c| 1.0 / (1.0 + c.max(0.0) / cost)),
        )
    } else {
        (0.0, 0.0, 0.0)
    };
    let actual_score = quality_score * (0.75 + 0.15 * time_efficiency + 0.10 * cost_efficiency);
    let raw_delta = k_factor * (actual_score - expected_score);
    let self_dealt = bounty.created_by_agent_id.as_deref() == Some(agent_id);
    // 正向收益才受防刷封顶；高于封顶线的旧分不回落，负向扣分始终保留。
    let (mmr_after, cap_reason) = if raw_delta <= 0.0 {
        ((state.mmr + raw_delta).max(0.0), None)
    } else if self_dealt {
        (state.mmr, Some("self_deal"))
    } else {
        let ceiling = match (diff_cap, arbitrated) {
            (Some(cap), false) if cap <= 1350.0 => Some((cap, "difficulty_cap")),
            (_, false) => Some((1350.0, "unarbitrated_cap")),
            (Some(cap), true) => Some((cap, "difficulty_cap")),
            (None, true) => None,
        };
        match ceiling {
            Some((cap, reason)) if state.mmr >= cap => (state.mmr, Some(reason)),
            Some((cap, reason)) if state.mmr + raw_delta > cap => (cap, Some(reason)),
            _ => (state.mmr + raw_delta, None),
        }
    };
    let verified = completed && arbitrated;
    let as_wins = state.verified_as_wins
        + i64::from(verified && matches!(effective_difficulty, Tier::A | Tier::S));
    let s_wins = state.verified_s_wins + i64::from(verified && effective_difficulty == Tier::S);
    let settled = state.settled_count + 1;
    let tier_after = assigned_tier(mmr_after, settled, as_wins, s_wins);
    // 先检查 A 的前置门槛，再检查 S；高分但 A/S 胜场不足时仍需先补足 A/S 胜场。
    let tier_gate_reason = if settled >= 5 && mmr_after >= 1500.0 && as_wins < 3 {
        Some("needs_as_wins")
    } else if settled >= 5 && mmr_after >= 1800.0 && s_wins < 5 {
        Some("needs_s_wins")
    } else {
        None
    };
    AgentRatingLedgerEntry {
        id: 0,
        agent_id: agent_id.into(),
        bounty_id: bounty.id.clone(),
        bounty_title: bounty.title.clone(),
        project_id: bounty.project_id.clone(),
        lease_id: bounty.lease_id,
        outcome,
        arbitrated,
        self_dealt,
        original_difficulty: bounty.difficulty,
        effective_difficulty,
        quality_grade: if arbitrated {
            bounty.quality_grade
        } else {
            None
        },
        rejection_count: bounty.rejection_count,
        quality_score,
        active_duration_ms: bounty.active_duration_ms,
        time_efficiency,
        completion_cost: bounty.completion_cost,
        cost_efficiency,
        actual_score,
        expected_score,
        k_factor,
        is_placement,
        raw_delta,
        applied_delta: if raw_delta > 0.0 && cap_reason.is_none() {
            raw_delta
        } else {
            mmr_after - state.mmr
        },
        mmr_before: state.mmr,
        mmr_after,
        tier_before: state.tier,
        tier_after,
        cap_reason: cap_reason.map(str::to_owned),
        tier_gate_reason: tier_gate_reason.map(str::to_owned),
        created_at: timestamp.into(),
    }
}

/// 函数职责：在调用方状态流转的写事务内更新隐藏分、段位、统计并插入单场流水。
/// 输入说明：bounty 必须保留本轮执行者、令牌及最终计时/成本/验收快照；失败回收清空前调用。
/// 约束条件：调用方先校验状态和 fencing，确保每轮只结算一次；不存在的历史执行者直接跳过。
/// 输出说明：不产生公开事件；任一步失败由外层事务连同任务状态一起回滚。
pub(super) fn settle(
    tx: &Transaction<'_>,
    bounty: &Bounty,
    outcome: RatingOutcome,
    arbitrated: bool,
    timestamp: &str,
) -> Result<()> {
    let Some(agent_id) = bounty.lease_agent_id.as_deref() else {
        return Ok(());
    };
    let state = tx.query_row(
        "SELECT mmr,tier,settled_count,verified_as_wins,verified_s_wins FROM agents WHERE id=?1",
        [agent_id],
        |r| Ok(RatingState { mmr: r.get(0)?, tier: enum_value(r, 1)?, settled_count: r.get(2)?, verified_as_wins: r.get(3)?, verified_s_wins: r.get(4)? }),
    ).optional()?;
    let Some(state) = state else {
        return Ok(());
    };
    let entry = calculate(bounty, agent_id, &state, outcome, arbitrated, timestamp);
    let completed = outcome == RatingOutcome::Completed;
    let verified = completed && arbitrated;
    tx.execute(
        "UPDATE agents SET mmr=?2,tier=?3,placement_completed=?4,settled_count=settled_count+1,completed_count=completed_count+?5,failed_count=failed_count+?6,verified_as_wins=verified_as_wins+?7,verified_s_wins=verified_s_wins+?8,updated_at=?9 WHERE id=?1",
        params![agent_id, entry.mmr_after, entry.tier_after.as_str(), state.settled_count + 1 >= 5, completed, !completed, verified && matches!(entry.effective_difficulty, Tier::A | Tier::S), verified && entry.effective_difficulty == Tier::S, timestamp],
    )?;
    tx.execute(
        "INSERT INTO agent_rating_ledger(agent_id,bounty_id,bounty_title,project_id,lease_id,outcome,arbitrated,self_dealt,original_difficulty,effective_difficulty,quality_grade,rejection_count,quality_score,active_duration_ms,time_efficiency,completion_cost,cost_efficiency,actual_score,expected_score,k_factor,is_placement,raw_delta,applied_delta,mmr_before,mmr_after,tier_before,tier_after,cap_reason,tier_gate_reason,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30)",
        params![entry.agent_id, entry.bounty_id, entry.bounty_title, entry.project_id, entry.lease_id, entry.outcome.as_str(), entry.arbitrated, entry.self_dealt, entry.original_difficulty.as_str(), entry.effective_difficulty.as_str(), entry.quality_grade.map(QualityGrade::as_str), entry.rejection_count, entry.quality_score, entry.active_duration_ms, entry.time_efficiency, entry.completion_cost, entry.cost_efficiency, entry.actual_score, entry.expected_score, entry.k_factor, entry.is_placement, entry.raw_delta, entry.applied_delta, entry.mmr_before, entry.mmr_after, entry.tier_before.as_str(), entry.tier_after.as_str(), entry.cap_reason, entry.tier_gate_reason, entry.created_at],
    )?;
    Ok(())
}

/// 仅管理员入口读取；显式列映射防止存储结构变化扩大 DTO 的披露范围。
pub(super) fn ledger_row(r: &Row<'_>) -> rusqlite::Result<AgentRatingLedgerEntry> {
    Ok(AgentRatingLedgerEntry {
        id: r.get(0)?,
        agent_id: r.get(1)?,
        bounty_id: r.get(2)?,
        bounty_title: r.get(3)?,
        project_id: r.get(4)?,
        lease_id: r.get(5)?,
        outcome: enum_value(r, 6)?,
        arbitrated: r.get(7)?,
        self_dealt: r.get(8)?,
        original_difficulty: enum_value(r, 9)?,
        effective_difficulty: enum_value(r, 10)?,
        quality_grade: optional_enum(r, 11)?,
        rejection_count: r.get(12)?,
        quality_score: r.get(13)?,
        active_duration_ms: r.get(14)?,
        time_efficiency: r.get(15)?,
        completion_cost: r.get(16)?,
        cost_efficiency: r.get(17)?,
        actual_score: r.get(18)?,
        expected_score: r.get(19)?,
        k_factor: r.get(20)?,
        is_placement: r.get(21)?,
        raw_delta: r.get(22)?,
        applied_delta: r.get(23)?,
        mmr_before: r.get(24)?,
        mmr_after: r.get(25)?,
        tier_before: enum_value(r, 26)?,
        tier_after: enum_value(r, 27)?,
        cap_reason: r.get(28)?,
        tier_gate_reason: r.get(29)?,
        created_at: r.get(30)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_thresholds_require_placement_and_verified_wins() {
        for (mmr, settled, as_wins, s_wins, expected) in [
            (2000.0, 4, 10, 10, AgentTier::Unranked),
            (0.0, 5, 0, 0, AgentTier::C),
            (1199.999, 5, 0, 0, AgentTier::C),
            (1200.0, 5, 0, 0, AgentTier::B),
            (1499.999, 5, 3, 0, AgentTier::B),
            (1500.0, 5, 2, 0, AgentTier::B),
            (1500.0, 5, 3, 0, AgentTier::A),
            (1799.999, 5, 5, 5, AgentTier::A),
            (1800.0, 5, 2, 0, AgentTier::B),
            (1800.0, 5, 3, 4, AgentTier::A),
            (1800.0, 5, 5, 5, AgentTier::S),
        ] {
            assert_eq!(assigned_tier(mmr, settled, as_wins, s_wins), expected);
        }
    }

    #[test]
    fn formula_clamps_negative_measurements_and_rating_floor() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let repo = Repository::open(file.path()).unwrap();
        let project = repo
            .create_project(serde_json::from_value(json!({"name":"formula"})).unwrap())
            .unwrap();
        let mut bounty = repo.create_bounty(serde_json::from_value(json!({"project_id":project.id,"title":"formula","difficulty":"C","assignee_mode":"any","status":"DRAFT"})).unwrap()).unwrap();
        bounty.active_duration_ms = -1;
        bounty.completion_cost = Some(-1.0);
        bounty.rejection_count = -1;
        bounty.quality_grade = Some(QualityGrade::Perfect);
        let state = RatingState {
            mmr: 1000.0,
            tier: AgentTier::Unranked,
            settled_count: 0,
            verified_as_wins: 0,
            verified_s_wins: 0,
        };
        let e = calculate(
            &bounty,
            "worker",
            &state,
            RatingOutcome::Completed,
            true,
            "now",
        );
        assert_eq!(e.quality_score, 1.0);
        assert_eq!(e.time_efficiency, 1.0);
        assert_eq!(e.cost_efficiency, 1.0);
        assert_eq!(e.actual_score, 1.0);
        assert_eq!(e.expected_score, 0.5);
        assert_eq!(e.raw_delta, 40.0);
        let state = RatingState { mmr: 0.0, ..state };
        let e = calculate(
            &bounty,
            "worker",
            &state,
            RatingOutcome::Timeout,
            false,
            "now",
        );
        assert!(e.raw_delta < 0.0);
        assert_eq!(e.mmr_after, 0.0);
        assert_eq!(e.applied_delta, 0.0);
        assert_eq!(e.cap_reason, None);
    }
}
