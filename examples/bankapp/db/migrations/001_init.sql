-- migration 001: initial tables (kept in sync with db/schema.sql)

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
