package com.harbor.booking;
import org.springframework.cloud.openfeign.FeignClient;
import org.springframework.web.bind.annotation.GetMapping;
@FeignClient(contextId = "invoiceLookup", name = "billing")
public interface BillingClient {
    @GetMapping("/invoices/{id}")
    String invoice(long id);
}
