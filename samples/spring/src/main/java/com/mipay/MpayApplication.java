package com.mipay;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.scheduling.annotation.EnableScheduling;

@SpringBootApplication
@EnableScheduling
public class MpayApplication {
    public static void main(String[] args) {
        SpringApplication.run(MpayApplication.class, args);
    }
}
