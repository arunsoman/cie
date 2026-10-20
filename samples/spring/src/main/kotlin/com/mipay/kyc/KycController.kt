package com.mipay.kyc

import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.RestController

@RestController
@RequestMapping("/kyc")
class KycController(
    private val kycService: KycService,
    private val riskClient: RiskClient
) {

    @GetMapping("/{userId}/status")
    fun status(@PathVariable userId: String): String {
        val state = kycService.stateOf(userId)
        if (state == "PENDING") {
            riskClient.flag(userId)
        } else if (state == "BLOCKED") {
            throw IllegalStateException("user blocked")
        }
        when (state) {
            "CLEAR" -> return "verified"
            "PENDING" -> return "pending review"
            else -> return "unknown $state"
        }
    }
}
