-- bankapp schema — the demo project's database shape.
-- This file is REAL SQL that CIE now reads: tables, views, and the
-- table-to-table edges (foreign keys) are drawn on the map.

CREATE TABLE accounts (
    id          BIGINT PRIMARY KEY,
    owner       VARCHAR(120) NOT NULL,
    balance     NUMERIC(14,2) NOT NULL DEFAULT 0,
    currency    VARCHAR(3) NOT NULL DEFAULT 'USD',
    opened_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE payments (
    id           BIGSERIAL PRIMARY KEY,
    account_id   BIGINT NOT NULL REFERENCES accounts(id),
    payee        VARCHAR(120) NOT NULL,
    amount       NUMERIC(14,2) NOT NULL,
    status       VARCHAR(20) NOT NULL DEFAULT 'pending',
    created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE audit_log (
    id          BIGSERIAL PRIMARY KEY,
    actor       VARCHAR(120) NOT NULL,
    action      VARCHAR(60) NOT NULL,
    payment_id  BIGINT REFERENCES payments(id),
    at          TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE VIEW pending_payments AS
    SELECT p.id, p.payee, p.amount, a.owner
    FROM payments p
    JOIN accounts a ON a.id = p.account_id
    WHERE p.status = 'pending';
