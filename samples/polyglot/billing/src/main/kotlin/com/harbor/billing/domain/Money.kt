package com.harbor.billing.domain

/** DDD value object — equality is by fields, never by identity. */
data class Money(val amountMinor: Int, val currency: String) {
  operator fun plus(other: Money): Money {
    if (currency != other.currency) {
      throw IllegalArgumentException("currency mismatch $currency vs ${other.currency}")
    }
    return Money(amountMinor + other.amountMinor, currency)
  }
}
