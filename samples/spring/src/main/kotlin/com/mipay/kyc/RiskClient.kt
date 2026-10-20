package com.mipay.kyc

import org.springframework.stereotype.Component

@Component
class RiskClient {
    fun flag(userId: String): Boolean {
        return true
    }
}
