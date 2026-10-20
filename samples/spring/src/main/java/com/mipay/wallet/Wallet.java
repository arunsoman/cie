package com.mipay.wallet;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;

@Entity
@Table(name = "wallets")
public class Wallet {
    @Id
    private Long id;
    private String owner;
    private Long balanceMinor;
    private String currency;

    public Long getId() { return id; }
    public void setId(Long id) { this.id = id; }
    public String getOwner() { return owner; }
    public Long getBalanceMinor() { return balanceMinor; }
    public String getCurrency() { return currency; }
}
