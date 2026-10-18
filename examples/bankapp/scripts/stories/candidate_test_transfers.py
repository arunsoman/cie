"""Tests for the candidate spending-limit feature.

Written BEFORE the implementation existed (the fix-and-recheck story:
US-121's first run fails for real), then passes once transfers.py lands.
"""

import pytest

from transfers import can_transfer, transfer


def test_under_limit_is_allowed():
    assert can_transfer(200, 1000) is True


def test_over_limit_is_refused():
    assert can_transfer(1200, 1000) is False


def test_transfer_returns_402_over_limit():
    out = transfer(1200, "bob", 1000)
    assert out["status"] == 402


def test_transfer_returns_200_under_limit():
    out = transfer(200, "alice", 1000)
    assert out["status"] == 200


def test_transfer_rejects_non_number():
    with pytest.raises(ValueError):
        transfer("abc", "carol", 1000)


def test_transfer_rejects_negative():
    with pytest.raises(ValueError):
        transfer(-5, "dave", 1000)
