package com.harbor.billing.infrastructure

import com.harbor.billing.application.BillingService
import com.harbor.billing.domain.Invoice
import org.springframework.web.bind.annotation.RestController
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.PostMapping
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable

/** The REST port — HTTP shape of the billing feature. */
@RestController
@RequestMapping("/invoices")
class BillingController(private val billingService: BillingService) {
  @PostMapping("/{id}/lines")
  fun addLine(@PathVariable id: Long, amountMinor: Int): Invoice {
    return billingService.addLine(id, amountMinor)
  }

  @PostMapping("/{id}/settle")
  fun settle(@PathVariable id: Long): Invoice {
    return billingService.settle(id)
  }

  @GetMapping("/{id}")
  fun get(@PathVariable id: Long): Invoice? {
    return billingService.get(id)
  }
}
