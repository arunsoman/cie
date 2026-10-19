package com.harbor.billing.domain

/** The aggregate root: state and the transitions live together. */
class Invoice(val id: Long, val customerId: Long) {
  private val lines = mutableListOf<Money>()
  var status: String = "draft"
    private set

  fun addLine(line: Money) {
    if (status == "settled") {
      throw IllegalStateException("settled invoices are immutable")
    }
    lines.add(line)
  }

  fun total(): Money {
    var sum = Money(0, "EUR")
    for (line in lines) {
      sum = sum + line
    }
    return sum
  }

  fun settle() {
    if (total().amountMinor <= 0) {
      throw IllegalStateException("nothing to settle")
    }
    status = "settled"
  }
}
