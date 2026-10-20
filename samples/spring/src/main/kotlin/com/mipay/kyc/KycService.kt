package com.mipay.kyc

import org.springframework.stereotype.Service

@Service
class KycService(
    private val kycRepository: KycRepository
) {
    fun stateOf(userId: String): String {
        val record = kycRepository.findByUserId(userId)
        if (record != null) {
            return record.status
        }
        return "UNKNOWN"
    }
}
