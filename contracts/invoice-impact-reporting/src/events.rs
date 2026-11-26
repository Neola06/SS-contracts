//! Events emitted by the invoice impact reporting contract.

use soroban_sdk::{Address, BytesN, Env, Symbol};
use crate::types::ReportType;

/// Emitted when an impact report is submitted by an issuer.
/// Event: ImpactReportSubmitted
pub fn impact_report_submitted(
    env: &Env,
    invoice_id: &BytesN<32>,
    issuer: &Address,
    metrics_hash: &BytesN<32>,
    report_type: ReportType,
) {
    let topics = (Symbol::new(env, "ImpactReportSubmitted"), invoice_id.clone());
    env.events().publish(
        topics,
        (issuer.clone(), metrics_hash.clone(), report_type as u32),
    );
}

/// Emitted when an impact report is verified by an admin.
/// Event: ImpactReportVerified
pub fn impact_report_verified(
    env: &Env,
    invoice_id: &BytesN<32>,
    admin: &Address,
) {
    let topics = (Symbol::new(env, "ImpactReportVerified"), invoice_id.clone());
    env.events().publish(topics, admin.clone());
}
