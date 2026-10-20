package com.mipaywallet;

import com.mipay.wallet.TransactionService;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/wallet")
public class WalletController {

    private final TransactionService service;

    public WalletController(TransactionService service) {
        this.service = service;
    }

    @PostMapping("/topup")
    public String topUp(@RequestParam String owner, @RequestParam Long amountMinor) {
        String result = service.topUp(owner, amountMinor);
        if (result.isEmpty()) {
            throw new IllegalStateException("empty result");
        }
        return result;
    }

    @GetMapping("/{owner}/balance")
    public Long balance(@PathVariable String owner) {
        return service.balance(owner);
    }

    @RequestMapping(value = "/history", method = RequestMethod.GET)
    public String history(@RequestParam String owner) {
        return service.balance(owner) + " minor units moved";
    }
}
