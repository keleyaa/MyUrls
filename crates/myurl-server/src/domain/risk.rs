use crate::config::Limits;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiskDecision {
    Allow,
    Challenge,
    Block,
}

#[derive(Clone, Copy, Debug)]
pub struct RiskInput<'a> {
    pub ten_minute_count: u64,
    pub daily_count: u64,
    pub risk_score: u64,
    pub limits: &'a Limits,
    pub challenge_enabled: bool,
    pub force_challenge: bool,
}

pub fn evaluate_risk(input: RiskInput<'_>) -> RiskDecision {
    if input.ten_minute_count > input.limits.hard_10m
        || input.daily_count > input.limits.hard_1d
        || input.risk_score >= input.limits.block_score
    {
        return RiskDecision::Block;
    }

    if input.challenge_enabled
        && (input.force_challenge
            || input.ten_minute_count > input.limits.direct_10m
            || input.risk_score >= input.limits.challenge_score)
    {
        return RiskDecision::Challenge;
    }

    RiskDecision::Allow
}

#[cfg(test)]
mod tests {
    use crate::config::Limits;

    use super::{RiskDecision, RiskInput, evaluate_risk};

    fn limits() -> Limits {
        Limits {
            direct_10m: 5,
            hard_10m: 20,
            hard_1d: 100,
            resolve_10s: 600,
            challenge_score: 3,
            block_score: 8,
        }
    }

    fn input(limits: &Limits) -> RiskInput<'_> {
        RiskInput {
            ten_minute_count: 1,
            daily_count: 1,
            risk_score: 0,
            limits,
            challenge_enabled: true,
            force_challenge: false,
        }
    }

    #[test]
    fn allows_low_risk_traffic() {
        let limits = limits();
        assert_eq!(evaluate_risk(input(&limits)), RiskDecision::Allow);
    }

    #[test]
    fn challenges_after_direct_threshold_or_challenge_score() {
        let limits = limits();
        let mut direct_limit = input(&limits);
        direct_limit.ten_minute_count = 6;
        assert_eq!(evaluate_risk(direct_limit), RiskDecision::Challenge);

        let mut challenge_score = input(&limits);
        challenge_score.risk_score = 3;
        assert_eq!(evaluate_risk(challenge_score), RiskDecision::Challenge);
    }

    #[test]
    fn blocks_hard_limits_and_block_score_before_challenge_rules() {
        let limits = limits();

        let mut ten_minute_hard_limit = input(&limits);
        ten_minute_hard_limit.ten_minute_count = 21;
        ten_minute_hard_limit.force_challenge = true;
        assert_eq!(evaluate_risk(ten_minute_hard_limit), RiskDecision::Block);

        let mut daily_hard_limit = input(&limits);
        daily_hard_limit.daily_count = 101;
        assert_eq!(evaluate_risk(daily_hard_limit), RiskDecision::Block);

        let mut block_score = input(&limits);
        block_score.risk_score = 8;
        assert_eq!(evaluate_risk(block_score), RiskDecision::Block);
    }

    #[test]
    fn keeps_threshold_comparisons_exactly_aligned_with_the_node_policy() {
        let limits = limits();

        let mut direct_boundary = input(&limits);
        direct_boundary.ten_minute_count = 5;
        assert_eq!(evaluate_risk(direct_boundary), RiskDecision::Allow);

        let mut hard_boundary = input(&limits);
        hard_boundary.ten_minute_count = 20;
        assert_eq!(evaluate_risk(hard_boundary), RiskDecision::Challenge);

        let mut daily_hard_boundary = input(&limits);
        daily_hard_boundary.daily_count = 100;
        assert_eq!(evaluate_risk(daily_hard_boundary), RiskDecision::Allow);
    }

    #[test]
    fn bypasses_challenge_only_when_the_provider_is_disabled() {
        let limits = limits();

        let mut disabled = input(&limits);
        disabled.ten_minute_count = 6;
        disabled.challenge_enabled = false;
        disabled.force_challenge = true;
        assert_eq!(evaluate_risk(disabled), RiskDecision::Allow);

        let mut forced = input(&limits);
        forced.force_challenge = true;
        assert_eq!(evaluate_risk(forced), RiskDecision::Challenge);

        let mut disabled_hard_limit = input(&limits);
        disabled_hard_limit.daily_count = 101;
        disabled_hard_limit.challenge_enabled = false;
        assert_eq!(evaluate_risk(disabled_hard_limit), RiskDecision::Block);
    }
}
