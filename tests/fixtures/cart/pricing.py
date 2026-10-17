"""Pricing rules."""

import store


def price_for(item, quantity):
    """Unit price times quantity."""
    return store.unit_price(item.sku) * quantity


def charge(cart, card):
    """Charge operation — must be preceded by authorization."""
    return True


def refund(cart, amount):
    """Refund operation — must be logged."""
    return True
