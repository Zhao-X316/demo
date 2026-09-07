//! Pure evidence aggregation. No database reads, writes or teacher decisions.
use super::{shanghai_date, ComputedMetric, EvidenceTarget, ProfilePolicy, ScopeNode};
use chrono::NaiveDate;
use std::collections::{BTreeMap, BTreeSet};

fn context_weight(context: &str) -> f64 {
    match context {
        "closed_book" => 1.0,
        "in_class" => 0.9,
        "homework" => 0.8,
        "open_book" => 0.6,
        "correction" => 0.6,
        _ => 0.5,
    }
}

fn freshness_weight(days: i64) -> f64 {
    if days <= 30 {
        1.0
    } else if days <= 90 {
        0.9
    } else if days <= 180 {
        0.75
    } else {
        0.6
    }
}

fn normalized_value(kind: &str, value: f64) -> f64 {
    if kind == "contradiction" {
        1.0 - value
    } else {
        value
    }
}

pub(super) fn compute_metric(
    node: &ScopeNode,
    evidence: Vec<EvidenceTarget>,
    policy: &ProfilePolicy,
    range_end: NaiveDate,
) -> ComputedMetric {
    if evidence.is_empty() {
        return ComputedMetric {
            target_type: node.target_type.clone(),
            target_public_id: node.public_id.clone(),
            target_title: node.title.clone(),
            mastery_score: None,
            status: "unassessed".into(),
            confidence_level: "none".into(),
            freshness: "none".into(),
            evidence_count: 0,
            independent_group_count: 0,
            distinct_date_count: 0,
            distinct_source_count: 0,
            last_evidence_at: None,
            source_breakdown: BTreeMap::new(),
            explanation: "所选范围内没有老师确认的逐点证据，不解释为零分或薄弱。".into(),
            evidence: Vec::new(),
        };
    }
    let mut dates = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut source_types = BTreeSet::new();
    let mut breakdown = BTreeMap::new();
    let mut groups: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut contributions = Vec::new();
    let mut last_evidence_at = None::<String>;
    for item in evidence {
        dates.insert(item.occurred_date);
        sources.insert(format!("{}:{}", item.source_ref_type, item.source_ref_id));
        source_types.insert(item.source_type.clone());
        *breakdown.entry(item.source_module.clone()).or_insert(0) += 1;
        let replaces_last = match last_evidence_at.as_ref() {
            Some(current) => item.occurred_at > *current,
            None => true,
        };
        if replaces_last {
            last_evidence_at = Some(item.occurred_at.clone());
        }
        let group_key = format!(
            "{}:{}:{}",
            item.source_ref_type, item.source_ref_id, item.occurred_date
        );
        let age_days = (range_end - item.occurred_date).num_days().max(0);
        let weight = (item.evidence_quality
            * context_weight(&item.assessment_context)
            * freshness_weight(age_days))
        .clamp(0.0, 1.0);
        groups
            .entry(group_key.clone())
            .or_default()
            .push((normalized_value(&item.evidence_kind, item.value), weight));
        contributions.push((item, group_key, weight));
    }
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    for values in groups.values() {
        let group_value = values
            .iter()
            .map(|(value, _)| *value)
            .fold(1.0_f64, f64::min);
        let group_weight = values
            .iter()
            .map(|(_, weight)| *weight)
            .fold(0.0_f64, f64::max);
        weighted_sum += group_value * group_weight;
        weight_sum += group_weight;
    }
    let mastery_score = if weight_sum > 0.0 {
        Some((weighted_sum / weight_sum).clamp(0.0, 1.0))
    } else {
        None
    };
    let eligible = groups.len() as i64 >= policy.min_independent_groups
        && dates.len() as i64 >= policy.min_distinct_dates
        && sources.len() as i64 >= policy.min_distinct_sources;
    let status = if !eligible {
        "insufficient_evidence"
    } else if mastery_score.is_some_and(|score| score < policy.needs_support_below) {
        "needs_support"
    } else if mastery_score.is_some_and(|score| score >= policy.stable_at_or_above) {
        "stable"
    } else {
        "developing"
    };
    let confidence_level = if !eligible {
        "low"
    } else if groups.len() >= 6 && dates.len() >= 3 && sources.len() >= 4 && source_types.len() >= 2
    {
        "high"
    } else {
        "medium"
    };
    let last_date = last_evidence_at
        .as_deref()
        .and_then(|value| shanghai_date(value).ok());
    let freshness = match last_date.map(|date| (range_end - date).num_days().max(0)) {
        Some(days) if days <= 30 => "fresh",
        Some(days) if days <= policy.freshness_days => "aging",
        Some(_) => "stale",
        None => "none",
    };
    let explanation = if !eligible {
        format!(
            "已有 {} 条证据，但独立组 {}/{}、日期 {}/{}、来源 {}/{}，暂不下掌握结论。",
            contributions.len(),
            groups.len(),
            policy.min_independent_groups,
            dates.len(),
            policy.min_distinct_dates,
            sources.len(),
            policy.min_distinct_sources
        )
    } else {
        format!(
            "基于 {} 个独立组、{} 个日期、{} 个来源；订正与开卷证据已降权。",
            groups.len(),
            dates.len(),
            sources.len()
        )
    };
    ComputedMetric {
        target_type: node.target_type.clone(),
        target_public_id: node.public_id.clone(),
        target_title: node.title.clone(),
        mastery_score,
        status: status.into(),
        confidence_level: confidence_level.into(),
        freshness: freshness.into(),
        evidence_count: contributions.len() as i64,
        independent_group_count: groups.len() as i64,
        distinct_date_count: dates.len() as i64,
        distinct_source_count: sources.len() as i64,
        last_evidence_at,
        source_breakdown: breakdown,
        explanation,
        evidence: contributions,
    }
}
