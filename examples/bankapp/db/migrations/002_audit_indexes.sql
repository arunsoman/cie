-- migration 002: audit trail + faster lookups

CREATE TABLE audit_log (
    id          BIGSERIAL PRIMARY KEY,
    actor       VARCHAR(120) NOT NULL,
    action      VARCHAR(60) NOT NULL,
    payment_id  BIGINT REFERENCES payments(id),
    at          TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_payments_account ON payments (account_id);
CREATE INDEX idx_payments_status ON payments (status);
