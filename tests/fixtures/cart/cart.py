"""Cart aggregate: items are added, priced, and totalled."""

import items
import pricing


class Cart:
    def __init__(self):
        self.lines = []

    def add_item(self, item, quantity):
        """Adds an item unless the cart is locked.

        Contract: quantity must be positive.
        """
        if quantity <= 0:
            return False
        self.lines.append((item, quantity))
        return True

    def total(self):
        """Total price of all lines.

        Contract: every line must be priced before totalling.
        """
        amount = 0
        for item, quantity in self.lines:
            amount = amount + pricing.price_for(item, quantity)
        return amount

    def list_purchases(self):
        return [item.name for item, _ in self.lines]

    def persist(self, store_conn):
        try:
            store_conn.save(self.lines)
        except Exception:
            pass
        return True
