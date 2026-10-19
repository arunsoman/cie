package com.harbor.billing.application

import com.harbor.billing.domain.Invoice
import com.harbor.billing.domain.InvoiceRepository
import com.harbor.billing.domain.Money
import org.springframework.stereotype.Service

/** Application service — orchestrates the aggregate through the port. */
@Service
class BillingService(private val invoiceRepository: InvoiceRepository) {
  fun addLine(invoiceId: Long, amountMinor: Int): Invoice {
    val invoice = invoiceRepository.byId(invoiceId)
      ?: throw NoSuchElementException("invoice $invoiceId not found")
    if (amountMinor <= 0) {
      throw IllegalArgumentException("line amount must be positive")
    }
    invoice.addLine(Money(amountMinor, "EUR"))
    return invoiceRepository.save(invoice)
  }

  fun get(invoiceId: Long): Invoice? {
    return invoiceRepository.byId(invoiceId)
  }

  fun settle(invoiceId: Long): Invoice {
    val invoice = invoiceRepository.byId(invoiceId)
      ?: throw NoSuchElementException("invoice $invoiceId not found")
    invoice.settle()
    return invoiceRepository.save(invoice)
  }
}
