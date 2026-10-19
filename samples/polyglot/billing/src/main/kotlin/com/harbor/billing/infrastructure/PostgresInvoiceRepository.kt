package com.harbor.billing.infrastructure

import com.harbor.billing.domain.Invoice
import com.harbor.billing.domain.InvoiceRepository
import org.springframework.stereotype.Repository

/** The adapter — the only place that knows about JDBC. */
@Repository
class PostgresInvoiceRepository : InvoiceRepository {
  override fun save(invoice: Invoice): Invoice {
    // write to the billing table
    return invoice
  }

  override fun byId(id: Long): Invoice? {
    // read from the billing table
    return null
  }
}
