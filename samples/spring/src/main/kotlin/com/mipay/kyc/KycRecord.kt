package com.mipay.kyc

import jakarta.persistence.Entity
import jakarta.persistence.Id
import jakarta.persistence.Table

@Entity
@Table(name = "kyc_records")
class KycRecord(
    @Id
    var id: Long = 0,
    var userId: String = "",
    var status: String = "PENDING"
)
