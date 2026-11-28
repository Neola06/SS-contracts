//! Error types for the invoice impact reporting contract.

use soroban_sdk::contracterror;

/// Errors that can occur during contract execution.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not authorized.
    Unauthorized = 3,
    /// Address is the zero address.
    InvalidAddress = 4,
    /// Invoice settlement record not found.
    InvoiceNotFound = 5,
    /// Invoice is not yet settled. Impact reports can only be submitted post-settlement.
    NotSettled = 6,
    /// An impact report already exists for this invoice.
    ReportAlreadyExists = 7,
    /// No impact report found for this invoice.
    ReportNotFound = 8,
    /// Impact report is already verified.
    AlreadyVerified = 9,
    /// Invalid report type specified.
    InvalidReportType = 10,
    /// Invalid metrics hash.
    InvalidMetricsHash = 11,
}
