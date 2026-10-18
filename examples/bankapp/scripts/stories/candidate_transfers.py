"""Transfers module — the walkthrough's candidate feature.

The spending limit starts at 500 (the first plan); the scope revision
raises it to 1000 and moves the reset flag into config. This file is the
REVISED candidate — exactly what the sandbox commit contains.
"""

LIMIT_DEFAULT = 1000  # revised from 500 when the scope changed (US-110)


def can_transfer(amount, limit: float = LIMIT_DEFAULT) -> bool:
    """must refuse transfers above the spending limit"""
    return amount <= limit


def transfer(amount, user: str, limit: float = LIMIT_DEFAULT) -> dict:
    """must refuse transfers above the spending limit; must reject bad input"""
    if not isinstance(amount, (int, float)) or isinstance(amount, bool):
        raise ValueError("amount must be a number")
    if amount <= 0:
        raise ValueError("amount must be positive")
    if not can_transfer(amount, limit):
        return {"status": 402, "reason": f"over limit: {amount} > {limit}"}
    return {"status": 200, "charged": amount, "user": user}
