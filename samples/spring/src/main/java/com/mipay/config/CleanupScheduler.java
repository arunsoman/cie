package com.mipay.config;

import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

@Component
public class CleanupScheduler {

    @Scheduled(cron = "0 0 3 * * *")
    public void nightlyCleanup() {
    }
}
