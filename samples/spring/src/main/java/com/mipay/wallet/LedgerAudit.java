package com.mipay.wallet;

import org.springframework.stereotype.Component;
import org.springframework.kafka.core.KafkaTemplate;

@Component
public class LedgerAudit {
    private final KafkaTemplate<String, String> kafka;

    public LedgerAudit(KafkaTemplate<String, String> kafka) {
        this.kafka = kafka;
    }

    public void record(String owner, Long amountMinor) {
        kafka.send("ledger-audit", owner + ":" + amountMinor);
    }
}
