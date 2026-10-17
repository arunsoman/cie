# Payments PRD

## Requirements

- The system must never double-charge a customer.
- The system must authorize every charge attempt.
- The system must log every refund.
- The API must return HTTP 200 on successful charge.
- The p99 latency of the charge endpoint must be under 50 ms.
- The system must be user-friendly.
- The system must support export of ledgers to CSV.
