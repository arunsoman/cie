package com.harbor.billing.domain

/** The port: the domain speaks this, infrastructure satisfies it. */
interface InvoiceRepository {
  fun save(invoice: Invoice): Invoice
  fun byId(id: Long): Invoice?
}
