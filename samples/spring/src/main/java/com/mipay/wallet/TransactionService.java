package com.mipay.wallet;

import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class TransactionService {

    private final WalletRepository wallets;
    private final LedgerAudit audit;

    public TransactionService(WalletRepository wallets, LedgerAudit audit) {
        this.wallets = wallets;
        this.audit = audit;
    }

    @Transactional
    public String topUp(String owner, Long amountMinor) {
        Wallet wallet = wallets.findByOwner(owner).orElseThrow(() -> new IllegalArgumentException("unknown wallet"));
        if (amountMinor <= 0) {
            throw new IllegalArgumentException("amount must be positive");
        } else {
            wallet.setBalanceMinor(wallet.getBalanceMinor() + amountMinor);
            wallets.save(wallet);
        }
        audit.record(owner, amountMinor);
        return "ok";
    }

    public Long balance(String owner) {
        Wallet wallet = wallets.findByOwner(owner).orElse(null);
        if (wallet == null) {
            throw new IllegalStateException("no wallet for " + owner);
        }
        return wallet.getBalanceMinor();
    }
}
